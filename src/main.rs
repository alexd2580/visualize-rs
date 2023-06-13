use std::{mem, rc::Rc};

use clap::Parser;

use error::Error;
use log::{error, info};

mod audio;
mod dft;
mod error;
mod ring_buffer;
mod thread_shared;
mod utils;
mod vulkan;
mod window;

/// Note the reverse drop order.
struct App {
    audio: audio::Audio,
    signal_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,
    signal_dft: dft::Dft,
    signal_dft_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,

    low_pass: audio::low_pass::LowPass,
    low_pass_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,
    low_pass_dft: dft::Dft,
    low_pass_dft_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,

    high_pass: audio::high_pass::HighPass,
    high_pass_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,
    high_pass_dft: dft::Dft,
    high_pass_dft_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,

    _intermediate: Rc<vulkan::multi_image::MultiImage>,
    vulkan: vulkan::Vulkan,
}

fn run_dft(
    buffer: &ring_buffer::RingBuffer<f32>,
    dft: &mut dft::Dft,
    dft_gpu: &vulkan::multi_buffer::MultiBuffer,
    vulkan: &vulkan::Vulkan,
) {
    buffer.write_to_buffer(dft.get_input_vec());
    dft.apply_hamming();
    dft.run_transform();
    dft.apply_scaling();
    dft.write_to_pointer(dft_gpu.mapped(vulkan.binding_index));
}

impl window::App for App {
    fn run_frame(&mut self) -> winit::event_loop::ControlFlow {
        let read_index = self.low_pass.write_index;

        let new_samples = self.audio.left.iter_at(read_index);
        for &x in new_samples {
            self.low_pass.sample(x);
            self.high_pass.sample(x);
        }

        let target = self.signal_gpu.mapped(0);
        self.audio.left.write_to_pointer(read_index, target);
        let target = self.signal_gpu.mapped(1);
        self.audio.left.write_to_pointer(read_index, target);
        let target = self.signal_gpu.mapped(2);
        self.audio.left.write_to_pointer(read_index, target);

        let target = self.low_pass_gpu.mapped(0);
        self.low_pass.write_to_pointer(read_index, target);
        let target = self.low_pass_gpu.mapped(1);
        self.low_pass.write_to_pointer(read_index, target);
        let target = self.low_pass_gpu.mapped(2);
        self.low_pass.write_to_pointer(read_index, target);

        let target = self.high_pass_gpu.mapped(0);
        self.high_pass.write_to_pointer(read_index, target);
        let target = self.high_pass_gpu.mapped(1);
        self.high_pass.write_to_pointer(read_index, target);
        let target = self.high_pass_gpu.mapped(2);
        self.high_pass.write_to_pointer(read_index, target);

        run_dft(
            &self.audio.left,
            &mut self.signal_dft,
            &self.signal_dft_gpu,
            &self.vulkan,
        );

        run_dft(
            &self.low_pass,
            &mut self.low_pass_dft,
            &self.low_pass_dft_gpu,
            &self.vulkan,
        );

        run_dft(
            &self.high_pass,
            &mut self.high_pass_dft,
            &self.high_pass_dft_gpu,
            &self.vulkan,
        );

        self.vulkan.run_frame()
    }

    fn handle_resize(&mut self, new_size: (u32, u32)) -> Result<(), error::Error> {
        self.vulkan.handle_resize(new_size)
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.vulkan.wait_idle();
    }
}

/// Run an audio visualizer.
#[derive(Parser)]
struct Args {
    /// The shader module path
    #[arg(short, long, num_args = 0.., default_value = "shaders/debug.comp")]
    shader_paths: Vec<std::path::PathBuf>,

    /// The DFT size
    #[arg(short, long, default_value = "2048")]
    dft_size: usize,

    /// The audio buffer size
    #[arg(short, long, default_value = "4")]
    audio_buffer_sec: usize,
}

fn run_main() -> Result<(), Error> {
    let args = Args::parse();

    let mut window = window::Window::new(1280, 1024)?;
    let mut vulkan = vulkan::Vulkan::new(&window, &args.shader_paths)?;

    let intermediate = vulkan.new_multi_image("intermediate")?;

    let sample_rate = 44100;
    let audio_buffer_size = sample_rate * args.audio_buffer_sec;
    let audio_buffer_bytes = (audio_buffer_size * mem::size_of::<f32>()) as u64;

    let audio = audio::Audio::new(audio_buffer_size)?;
    let signal_gpu = vulkan.new_multi_buffer("signal", audio_buffer_bytes)?;

    let low_pass = audio::low_pass::LowPass::new(audio_buffer_size, 0.02);
    let low_pass_gpu = vulkan.new_multi_buffer("low_pass", audio_buffer_bytes)?;

    let high_pass = audio::high_pass::HighPass::new(audio_buffer_size, 0.1);
    let high_pass_gpu = vulkan.new_multi_buffer("high_pass", audio_buffer_bytes)?;

    let dft_size = args.dft_size as f32;
    let dft_window_per_s = audio.sample_rate as f32 / dft_size;
    let dft_min_fq = dft_window_per_s * 1f32;
    let dft_max_fq = dft_window_per_s * dft_size / 2f32;
    info!("DFT can analyze frequencies in the range: {dft_min_fq} hz - {dft_max_fq} hz");

    let dft_result_size = dft::Dft::output_byte_size(args.dft_size) as u64;

    let signal_dft = dft::Dft::new(args.dft_size);
    let signal_dft_gpu = vulkan.new_multi_buffer("signal_dft", dft_result_size)?;

    let low_pass_dft = dft::Dft::new(args.dft_size);
    let low_pass_dft_gpu = vulkan.new_multi_buffer("low_pass_dft", dft_result_size)?;

    let high_pass_dft = dft::Dft::new(args.dft_size);
    let high_pass_dft_gpu = vulkan.new_multi_buffer("high_pass_dft", dft_result_size)?;

    log::info!("Running...");
    {
        let mut app = App {
            audio,
            signal_gpu,
            signal_dft,
            signal_dft_gpu,
            low_pass,
            low_pass_gpu,
            low_pass_dft,
            low_pass_dft_gpu,
            high_pass,
            high_pass_gpu,
            high_pass_dft,
            high_pass_dft_gpu,
            _intermediate: intermediate,
            vulkan,
        };
        window.run_main_loop(&mut app);
    }

    Ok(())
}

fn main() {
    simple_logger::init_with_level(log::Level::Debug).unwrap();
    log::info!("Initializing...");
    if let Err(err) = run_main() {
        error!("{}", err);
    }
    log::info!("Terminating...");
}
