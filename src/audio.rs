use log::debug;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

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

fn init_input_stream(host: &cpal::Host, desired_sample_rate: u32) -> cpal::Stream {
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

    fn read<T: cpal::Sample>(data: &[T], _: &cpal::InputCallbackInfo) {
        for sample in data.iter() {}
    }

    device
        .build_input_stream(&config, read::<f32>, print_error)
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

    fn write_silence<T: cpal::Sample>(data: &mut [T], _: &cpal::OutputCallbackInfo) {
        for sample in data.iter_mut() {
            *sample = cpal::Sample::from(&0.0);
        }
    }

    device
        .build_output_stream(&config, write_silence::<f32>, print_error)
        .unwrap()
}

pub struct Audio {
    host: cpal::Host,

    sample_rate: u32,

    input_stream: cpal::Stream,
    output_stream: cpal::Stream,
}

impl Audio {
    pub fn new() -> Self {
        let host = cpal::default_host();

        let sample_rate = 44100;

        debug!("Initializing audio streams");
        let input_stream = init_input_stream(&host, sample_rate);
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
