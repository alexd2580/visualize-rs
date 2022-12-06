use std::sync::Arc;

use log::debug;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::audio_buffer::{AudioBuffer, AUDIO_BUFFER_SIZE};

fn choose_stream_config<ConfigsIter: Iterator<Item = cpal::SupportedStreamConfigRange>>(
    // This is a newtype for a `range` iterator.
    configs_iter: ConfigsIter,
    num_channels: u16,
    sample_rate: cpal::SampleRate,
    sample_format: cpal::SampleFormat,
) -> Option<cpal::StreamConfig> {
    configs_iter
        .into_iter()
        .find_map(|range: cpal::SupportedStreamConfigRange| {
            let format = range.sample_format() == sample_format;
            let channels = range.channels() == num_channels;
            let min_sample_rate = range.min_sample_rate() <= sample_rate;
            let max_sample_rate = range.max_sample_rate() >= sample_rate;
            if format && channels && min_sample_rate && max_sample_rate {
                Some(range.with_sample_rate(sample_rate))
            } else {
                None
            }
        })
        .map(cpal::SupportedStreamConfig::into)
}

fn init_input_stream(
    host: &cpal::Host,
    desired_sample_rate: u32,
    buffer: Arc<AudioBuffer>,
) -> cpal::Stream {
    let device = host.default_input_device().unwrap();
    let desired_sample_format = cpal::SampleFormat::F32;
    let config = choose_stream_config(
        device.supported_input_configs().unwrap(),
        2,
        cpal::SampleRate(desired_sample_rate),
        desired_sample_format,
    )
    .unwrap();

    let print_error = |err| eprintln!("Audio input error: {}", err);

    let read = move |samples: &[f32], _: &cpal::InputCallbackInfo| buffer.write_samples(samples);

    device
        .build_input_stream(&config, read, print_error)
        .unwrap()
}

fn init_output_stream(host: &cpal::Host, desired_sample_rate: u32) -> cpal::Stream {
    let device = host.default_output_device().unwrap();
    let desired_sample_format = cpal::SampleFormat::F32;
    let config = choose_stream_config(
        device.supported_output_configs().unwrap(),
        2,
        cpal::SampleRate(desired_sample_rate),
        desired_sample_format,
    )
    .unwrap();

    let print_error = |err| eprintln!("Audio output error: {}", err);

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

    device
        .build_output_stream(&config, write_silence, print_error)
        .unwrap()
}

pub struct Audio {
    host: cpal::Host,

    sample_rate: u32,

    input_stream: cpal::Stream,
    output_stream: cpal::Stream,
}

impl Audio {
    pub fn new(ring_buffer: &Arc<AudioBuffer>) -> Self {
        let host = cpal::default_host();

        let sample_rate = 44100;

        debug!("Initializing audio streams");
        let input_stream = init_input_stream(&host, sample_rate, Arc::clone(ring_buffer));
        let output_stream = init_output_stream(&host, sample_rate);

        debug!("Running audio streams");
        input_stream.play().unwrap();
        output_stream.play().unwrap();

        Audio {
            host,
            sample_rate,
            input_stream,
            output_stream,
        }
    }
}
