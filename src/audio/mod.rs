use std::ops::Deref;

use log::{debug, error, warn};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::{error::Error, thread_shared::ThreadShared};

pub mod high_pass;
pub mod low_pass;
mod routing;
mod stereo;
mod virtual_sink;

fn choose_stream_config<ConfigsIter: Iterator<Item = cpal::SupportedStreamConfigRange>>(
    // This is a newtype for a `range` iterator.
    configs_iter: ConfigsIter,
    num_channels: u16,
    sample_rate: cpal::SampleRate,
    sample_format: cpal::SampleFormat,
) -> Result<cpal::StreamConfig, Error> {
    configs_iter
        .into_iter()
        .find_map(|range: cpal::SupportedStreamConfigRange| {
            let format = range.sample_format() == sample_format;
            let channels = range.channels() == num_channels;
            let min_sample_rate = range.min_sample_rate() <= sample_rate;
            let max_sample_rate = range.max_sample_rate() >= sample_rate;
            if format && channels && min_sample_rate && max_sample_rate {
                let range = range.with_sample_rate(sample_rate);
                Some(cpal::SupportedStreamConfig::into(range))
            } else {
                None
            }
        })
        .ok_or_else(|| Error::Local("Failed to choose stream config".to_owned()))
}

pub struct Audio {
    host: cpal::Host,
    pub sample_rate: usize,
    ring_buffer: ThreadShared<stereo::Stereo>,

    _virtual_sink: Option<virtual_sink::VirtualSink>,
    routing: Option<routing::Routing>,
    default_sink: Option<pulsectl::controllers::types::DeviceInfo>,

    input_stream: Option<cpal::Stream>,
    output_stream: Option<cpal::Stream>,
}

impl Deref for Audio {
    type Target = stereo::Stereo;

    fn deref(&self) -> &Self::Target {
        self.ring_buffer.read()
    }
}

impl Audio {
    pub fn buffer_size(&self) -> usize {
        self.ring_buffer.read().left.size
    }

    fn input_device(&self) -> Result<cpal::Device, Error> {
        self.host
            .default_input_device()
            .ok_or_else(|| Error::Local("Failed to get default input device.".to_owned()))
    }

    fn run_input_stream<Callback>(&self, callback: Callback) -> Result<cpal::Stream, Error>
    where
        Callback: FnMut(&[f32], &cpal::InputCallbackInfo) + Send + 'static,
    {
        let device = self.input_device()?;
        let config = choose_stream_config(
            device.supported_input_configs()?,
            2,
            cpal::SampleRate(u32::try_from(self.sample_rate).unwrap()),
            cpal::SampleFormat::F32,
        )?;

        let print_error = |err| eprintln!("Audio input error: {err}");
        let stream = device.build_input_stream(&config, callback, print_error)?;
        stream.play()?;
        Ok(stream)
    }

    fn init_input_stream(&mut self) -> Result<(), Error> {
        debug!("Initializing input stream");
        let buffer = self.ring_buffer.clone();
        let read = move |samples: &[f32], _: &cpal::InputCallbackInfo| {
            buffer.write().write_samples(samples);
        };
        self.input_stream = Some(self.run_input_stream(read)?);
        Ok(())
    }

    fn output_device(&self) -> Result<cpal::Device, Error> {
        self.host
            .default_output_device()
            .ok_or_else(|| Error::Local("Failed to get default output device.".to_owned()))
    }

    fn run_output_stream<Callback>(&self, callback: Callback) -> Result<cpal::Stream, Error>
    where
        Callback: FnMut(&mut [f32], &cpal::OutputCallbackInfo) + Send + 'static,
    {
        let device = self.output_device()?;
        let config = choose_stream_config(
            device.supported_output_configs()?,
            2,
            cpal::SampleRate(u32::try_from(self.sample_rate).unwrap()),
            cpal::SampleFormat::F32,
        )?;

        let print_error = |err| eprintln!("Audio output error: {err}");
        let stream = device.build_output_stream(&config, callback, print_error)?;
        stream.play()?;
        Ok(stream)
    }

    fn init_output_stream(&mut self) -> Result<(), Error> {
        debug!("Initializing output stream");
        let buffer = self.ring_buffer.clone();
        let mut read_index = 0;
        let mut insert_delay_samples = 5000;
        // If we want to delay the input stream, then we need to be able to do so.
        assert!(2 * insert_delay_samples < buffer.read().left.size);

        // TODO handle callbackinfo
        let write = move |mut data: &mut [f32], _callback_info: &cpal::OutputCallbackInfo| {
            let mut data_num_samples = data.len() / 2;

            if insert_delay_samples > 0 {
                let how_many = insert_delay_samples.min(data_num_samples);
                data[0..2 * how_many].fill(0.0);
                insert_delay_samples -= how_many;

                data = &mut data[2 * how_many..];
                data_num_samples = data.len() / 2;
            }

            // No more delay samples.
            let stereo::Stereo { left, right } = buffer.read();
            let read_buf_size = left.size;

            // Unfurled write index.
            let write_index = if left.write_index < read_index {
                left.write_index + read_buf_size
            } else {
                left.write_index
            };
            // Where would i end up in the read_buffer after reading `data_num_samples`.
            let read_end_index = read_index + data_num_samples;
            if read_end_index > write_index {
                let underrun = read_end_index - write_index;
                warn!("Audio sample underrun by {underrun} samples, filling with 0es.");
                let fill_samples = underrun.min(data_num_samples);
                data[..2 * fill_samples].fill(0.0);
                data = &mut data[2 * fill_samples..];
                data_num_samples = data.len() / 2;
            }

            if read_index + data_num_samples <= read_buf_size {
                // Case where we can read one continuous stretch of samples.
                for sample_index in 0..data_num_samples {
                    let buffer_index = read_index + sample_index;
                    data[2 * sample_index] = left.data[buffer_index];
                    data[2 * sample_index + 1] = right.data[buffer_index];
                }

                // Wrap read_index around.
                read_index = if read_index + data_num_samples == read_buf_size {
                    0
                } else {
                    read_index + data_num_samples
                };
            } else {
                // Case where sample stretch wraps around in ring buffer.
                let pt1 = left.size - read_index;
                let pt2 = data_num_samples - pt1;
                assert!(pt1 + pt2 == data_num_samples);

                for sample_index in 0..pt1 {
                    let buffer_index = read_index + sample_index;
                    data[2 * sample_index] = left.data[buffer_index];
                    data[2 * sample_index + 1] = right.data[buffer_index];
                }

                data = &mut data[2 * pt1..];
                data_num_samples = data.len() / 2;
                assert!(data_num_samples == pt2);

                for sample_index in 0..pt2 {
                    let buffer_index = sample_index;
                    data[2 * sample_index] = left.data[buffer_index];
                    data[2 * sample_index + 1] = right.data[buffer_index];
                }

                // Move read index to end of read part.
                read_index = pt2;
            }
        };

        self.output_stream = Some(self.run_output_stream(write)?);
        Ok(())
    }

    fn routing(&mut self) -> &mut routing::Routing {
        self.routing.as_mut().unwrap()
    }

    pub fn new(seconds: u32, delayed_echo: bool) -> Result<Self, Error> {
        let host = cpal::default_host();

        // Create ring buffer.
        let sample_rate = 44100;
        let buffer_size = usize::try_from(seconds).unwrap() * sample_rate; // TODO
        let ring_buffer = ThreadShared::new(stereo::Stereo::new(buffer_size));

        // Initialize virtual sink.
        let virtual_sink_name = "visualize-rs".to_owned();
        let virtual_sink = delayed_echo
            .then(|| virtual_sink::VirtualSink::new(virtual_sink_name.clone()))
            .transpose()?;

        let mut routing = delayed_echo.then(routing::Routing::new).transpose()?;
        let default_sink = delayed_echo
            .then(|| routing.as_mut().unwrap().default_sink())
            .transpose()?;

        let mut audio = Audio {
            host,
            sample_rate,
            ring_buffer,
            _virtual_sink: virtual_sink,
            routing,
            default_sink,
            input_stream: None,
            output_stream: None,
        };

        // Start the streams.
        // TODO redirect default devices etcetc
        audio.init_input_stream()?;
        if delayed_echo {
            audio.init_output_stream()?;

            let routing = audio.routing();
            let default_output = routing.default_sink()?;
            let (virtual_sink, virtual_monitor) =
                routing.get_sink_and_monitor_device(&virtual_sink_name)?;
            let (visualizer_record, visualizer_playback) =
                routing.get_record_and_playback_streams(&virtual_sink_name)?;

            if let (Some(visualizer_record), Some(visualizer_playback)) =
                (visualizer_record, visualizer_playback)
            {
                debug!(
                    "Visualizer record: {}; playback: {}",
                    visualizer_record.name.as_ref().unwrap(),
                    visualizer_playback.name.as_ref().unwrap()
                );
                routing.set_default_sink_device(&virtual_sink)?;
                routing.set_record_input(&visualizer_record, &virtual_monitor)?;
                routing.set_playback_output(&visualizer_playback, &default_output)?;
            } else {
                warn!("Can't find app '{virtual_sink_name}' in record or playback streams");
            }
        }

        Ok(audio)
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        if let (Some(routing), Some(default_sink)) = (&mut self.routing, &self.default_sink) {
            let result = routing.set_default_sink_device(default_sink);
            if let Err(error) = result {
                error!(
                    "Failed to set default sink back to {}: {error}",
                    default_sink.name.as_ref().unwrap()
                );
            }
        }
    }
}
