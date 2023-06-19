use std::ops::Deref;

use log::debug;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::{error::Error, thread_shared::ThreadShared};

pub mod high_pass;
pub mod low_pass;
mod stereo;

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

fn init_input_stream(
    host: &cpal::Host,
    desired_sample_rate: usize,
    buffer: &ThreadShared<stereo::Stereo>,
) -> Result<cpal::Stream, Error> {
    let buffer = buffer.clone();

    let device = host
        .default_input_device()
        .ok_or_else(|| Error::Local("Failed to get default input device.".to_owned()))?;
    let desired_sample_format = cpal::SampleFormat::F32;
    let config = choose_stream_config(
        device.supported_input_configs()?,
        2,
        cpal::SampleRate(u32::try_from(desired_sample_rate).unwrap()),
        desired_sample_format,
    )?;

    let print_error = |err| eprintln!("Audio input error: {err}");

    let read =
        move |samples: &[f32], _: &cpal::InputCallbackInfo| buffer.write().write_samples(samples);

    Ok(device.build_input_stream(&config, read, print_error)?)
}

fn init_output_stream(
    host: &cpal::Host,
    desired_sample_rate: usize,
) -> Result<cpal::Stream, Error> {
    let device = host
        .default_output_device()
        .ok_or_else(|| Error::Local("Failed to get default output device.".to_owned()))?;
    let desired_sample_format = cpal::SampleFormat::F32;
    let config = choose_stream_config(
        device.supported_output_configs()?,
        2,
        cpal::SampleRate(u32::try_from(desired_sample_rate).unwrap()),
        desired_sample_format,
    )?;

    let print_error = |err| eprintln!("Audio output error: {err}");

    let mut samples_written = 0;
    let write_silence = move |data: &mut [f32], _callback_info: &cpal::OutputCallbackInfo| {
        let frequency = 200.0;
        for (index, channels) in data.chunks_mut(2).enumerate() {
            let x_s = (samples_written + index) as f32 / 44100.0
                * (2.0 * std::f32::consts::PI)
                * frequency;
            channels[0] = x_s.sin() / 10.0;
            channels[1] = x_s.sin() / 10.0;
        }
        samples_written += data.len() / 2;
    };

    Ok(device.build_output_stream(&config, write_silence, print_error)?)
}

pub struct Audio {
    ring_buffer: ThreadShared<stereo::Stereo>,

    _host: cpal::Host,

    pub sample_rate: usize,

    _input_stream: cpal::Stream,
    _output_stream: Option<cpal::Stream>,
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

    pub fn new(seconds: u32, echo: bool) -> Result<Self, Error> {
        let sample_rate = 44100;
        let buffer_size = usize::try_from(seconds).unwrap() * sample_rate; // TODO
        let ring_buffer = ThreadShared::new(stereo::Stereo::new(buffer_size));

        let host = cpal::default_host();

        debug!("Initializing audio streams");
        let input_stream = init_input_stream(&host, sample_rate, &ring_buffer)?;
        let output_stream = if echo {
            Some(init_output_stream(&host, sample_rate)?)
        } else {
            None
        };

        debug!("Running audio streams");
        input_stream.play()?;
        output_stream.as_ref().map(cpal::Stream::play).transpose()?;

        Ok(Audio {
            ring_buffer,
            _host: host,
            sample_rate,
            _input_stream: input_stream,
            _output_stream: output_stream,
        })
    }
}
