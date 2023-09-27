use std::ops::Deref;

use log::{debug, error, warn};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::{error::{Error, VResult}, thread_shared::ThreadShared};

use self::{routing::Routing, stereo::Stereo};

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

struct Cpal {
    host: cpal::Host,
    pub sample_rate: usize,
}

impl Cpal {
    fn cpal_sample_rate(&self) -> cpal::SampleRate {
        cpal::SampleRate(u32::try_from(self.sample_rate).unwrap())
    }

    fn default_input_device(&self) -> Result<cpal::Device, Error> {
        self.host
            .default_input_device()
            .ok_or_else(|| Error::Local("Failed to get default input device.".to_owned()))
    }

    fn run_input_stream<Callback>(
        &self,
        device: cpal::Device,
        callback: Callback,
    ) -> Result<cpal::Stream, Error>
    where
        Callback: FnMut(&[f32], &cpal::InputCallbackInfo) + Send + 'static,
    {
        let config = choose_stream_config(
            device.supported_input_configs()?,
            2,
            self.cpal_sample_rate(),
            cpal::SampleFormat::F32,
        )?;

        let print_error = |err| eprintln!("Audio input error: {err}");
        let stream = device.build_input_stream(&config, callback, print_error)?;
        stream.play()?;
        Ok(stream)
    }

    fn default_output_device(&self) -> Result<cpal::Device, Error> {
        self.host
            .default_output_device()
            .ok_or_else(|| Error::Local("Failed to get default output device.".to_owned()))
    }

    fn run_output_stream<Callback>(
        &self,
        device: cpal::Device,
        callback: Callback,
    ) -> Result<cpal::Stream, Error>
    where
        Callback: FnMut(&mut [f32], &cpal::OutputCallbackInfo) + Send + 'static,
    {
        let config = choose_stream_config(
            device.supported_output_configs()?,
            2,
            self.cpal_sample_rate(),
            cpal::SampleFormat::F32,
        )?;

        let print_error = |err| eprintln!("Audio output error: {err}");
        let stream = device.build_output_stream(&config, callback, print_error)?;
        stream.play()?;
        Ok(stream)
    }

    pub fn new() -> Self {
        Cpal {
            host: cpal::default_host(),
            sample_rate: 44100,
        }
    }
}

struct DelayedOutput {
    routing: routing::Routing,
    default_sink: routing::types::DeviceInfo,
    #[allow(dead_code)]
    virtual_output_device: virtual_sink::VirtualSink,
    #[allow(dead_code)]
    output_stream: cpal::Stream,
}

impl DelayedOutput {
    fn init_output_stream(
        cpal: &Cpal,
        device: cpal::Device,
        ring_buffer: &ThreadShared<Stereo>,
    ) -> Result<cpal::Stream, Error> {
        debug!("Initializing output stream");
        let buffer = ring_buffer.clone();
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
            let stereo::Stereo { left, right, .. } = buffer.read();
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

        cpal.run_output_stream(device, write)
    }

    fn new(cpal: &Cpal, ring_buffer: &ThreadShared<Stereo>) -> Result<Self, Error> {
        let mut routing = routing::Routing::new()?;

        // Store the current output device.
        let default_sink = routing.get_default_sink_device()?;

        // Initialize virtual sink.
        let virtual_device_name = "Audio collector";
        let virtual_output_device = virtual_sink::VirtualSink::new(virtual_device_name.to_owned())?;

        // routing.print()?;

        let virtual_sink = routing.get_sink_device_by_name(virtual_device_name)?;
        let virtual_monitor_name = format!("{virtual_device_name}.monitor");
        let virtual_monitor = routing.get_source_device_by_name(&virtual_monitor_name)?;

        // Run the delayed output stream on the current default device.
        let output_stream = {
            let write_device = cpal.default_output_device()?;
            DelayedOutput::init_output_stream(cpal, write_device, ring_buffer)?
        };

        let app_name = "visualize-rs";
        let record_streams = routing.list_record_applications()?;
        let visualizer_record = Routing::find_stream_by_name(&record_streams, app_name);
        let playback_streams = routing.list_playback_applications()?;
        let visualizer_playback = Routing::find_stream_by_name(&playback_streams, app_name);

        if let (Some(visualizer_record), Some(visualizer_playback)) =
            (visualizer_record, visualizer_playback)
        {
            let vis_rec_name = visualizer_record.name.as_ref().unwrap();
            debug!("App record stream: {vis_rec_name}");
            let vis_out_name = visualizer_playback.name.as_ref().unwrap();
            debug!("App output stream: {vis_out_name}");

            routing.set_default_sink_device(&virtual_sink.name.as_ref().unwrap())?;
            routing.set_record_input(&visualizer_record, &virtual_monitor)?;
            routing.set_playback_output(&visualizer_playback, &default_sink)?;
        } else {
            warn!("Can't find app '{app_name}' in record or playback streams");
        }

        Ok(DelayedOutput {
            routing,
            default_sink,
            virtual_output_device,
            output_stream,
        })
    }
}

impl Drop for DelayedOutput {
    fn drop(&mut self) {
        let sink_name = self.default_sink.name.as_ref().unwrap();
        let result = self.routing.set_default_sink_device(sink_name);
        if let Err(error) = result {
            error!("Failed to set default sink back to {sink_name}: {error}",);
        }
    }
}

pub struct Audio {
    cpal: Cpal,
    ring_buffer: ThreadShared<stereo::Stereo>,
    #[allow(dead_code)]
    input_stream: cpal::Stream,
    #[allow(dead_code)]
    delayed_output: Option<DelayedOutput>,
}

impl Deref for Audio {
    type Target = stereo::Stereo;

    fn deref(&self) -> &Self::Target {
        self.ring_buffer.read()
    }
}

impl Audio {
    pub fn sample_rate(&self) -> usize {
        self.cpal.sample_rate
    }

    pub fn buffer_size(&self) -> usize {
        self.ring_buffer.read().left.size
    }

    fn init_input_stream(
        cpal: &Cpal,
        device: cpal::Device,
        ring_buffer: &ThreadShared<Stereo>,
    ) -> Result<cpal::Stream, Error> {
        debug!("Initializing input stream");
        let buffer = ring_buffer.clone();
        let read = move |samples: &[f32], _: &cpal::InputCallbackInfo| {
            buffer.write().write_samples(samples);
        };
        cpal.run_input_stream(device, read)
    }

    pub fn new(seconds: u32, delayed_echo: bool) -> VResult<Self> {
        let cpal = Cpal::new();

        // TODO todo what? unwrap? sample rate?
        let buffer_size = usize::try_from(seconds).unwrap() * cpal.sample_rate;
        let ring_buffer = ThreadShared::new(stereo::Stereo::new(buffer_size));

        let read_device = cpal.default_input_device()?;
        let input_stream = Audio::init_input_stream(&cpal, read_device, &ring_buffer)?;
        let delayed_output = delayed_echo
            .then(|| DelayedOutput::new(&cpal, &ring_buffer))
            .transpose()?;

        Ok(Audio {
            cpal,
            ring_buffer,
            input_stream,
            delayed_output,
        })
    }
}
