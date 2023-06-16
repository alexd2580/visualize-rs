use std::{collections::HashMap, mem, rc::Rc, time};

use ash::vk;
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

struct Timer {
    alpha: f32,
    last_section_end: time::Instant,
    section_order: Vec<*const u8>,
    sections: HashMap<*const u8, (&'static str, time::Duration)>,
}

impl Timer {
    fn new(alpha: f32) -> Timer {
        Timer {
            alpha,
            last_section_end: time::Instant::now(),
            section_order: Vec::new(),
            sections: HashMap::new(),
        }
    }

    fn start(&mut self) {
        self.last_section_end = time::Instant::now();
    }

    fn section(&mut self, name: &'static str) {
        let delta = self.last_section_end.elapsed();
        let key = name.as_ptr();
        match self.sections.get_mut(&key) {
            None => {
                self.sections.insert(key, (name, delta));
                self.section_order.push(key);
            }
            Some((_, avg_delta)) => {
                *avg_delta = avg_delta.mul_f32(self.alpha) + delta.mul_f32(1f32 - self.alpha);
            }
        }

        self.start();
    }

    fn print(&self) {
        info!("Timings");
        for key in &self.section_order {
            let (name, value) = self.sections.get(key).unwrap();
            let value = value.as_secs_f32();
            info!("  {name: <20} {value:.4}s");
        }
    }
}

/// Note the reverse drop order.
struct Visualizer {
    available_samples: usize,
    avg_available_samples: f32,
    avg_available_samples_alpha: f32,

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

    images: Vec<Rc<vulkan::multi_image::MultiImage>>,

    vulkan: vulkan::Vulkan,

    timer: Timer,
}

fn run_dft(
    buffer: &ring_buffer::RingBuffer<f32>,
    dft: &mut dft::Dft,
    dft_gpu: &vulkan::multi_buffer::MultiBuffer,
) {
    buffer.write_to_buffer(dft.get_input_vec());
    dft.apply_hamming();
    dft.run_transform();
    dft.write_to_pointer(dft_gpu.mapped(0));
}

impl window::App for Visualizer {
    fn run_frame(&mut self) -> winit::event_loop::ControlFlow {
        self.timer.section("Outside of loop");

        let read_index = self.low_pass.write_index;
        let write_index = self.audio.left.write_index;
        let buf_size = self.audio.left.data.len();

        // Total available samples.
        let available_samples = if write_index < read_index {
            write_index + buf_size - read_index
        } else {
            write_index - read_index
        };

        // New available in this frame.
        let new_available = available_samples - self.available_samples;
        self.avg_available_samples = self.avg_available_samples * self.avg_available_samples_alpha
            + new_available as f32 * (1f32 - self.avg_available_samples_alpha);

        // `+5` makes it so that i try to display more frames without lagging behind too much.
        // This is a magic number, might be different for different FPS.
        let consume_samples = (self.avg_available_samples as usize + 5).min(available_samples);
        self.available_samples = available_samples - consume_samples;

        let write_index = (read_index + consume_samples) % buf_size;

        if write_index < read_index {
            for index in read_index..buf_size {
                let x = self.audio.left.data[index];
                self.low_pass.sample(x);
                self.high_pass.sample(x);
            }
            for index in 0..write_index {
                let x = self.audio.left.data[index];
                self.low_pass.sample(x);
                self.high_pass.sample(x);
            }
        } else {
            for index in read_index..write_index {
                let x = self.audio.left.data[index];
                self.low_pass.sample(x);
                self.high_pass.sample(x);
            }
        }

        self.timer.section("Filters");

        let target = self.signal_gpu.mapped(0);
        self.audio
            .left
            .write_to_pointer(read_index, write_index, target);

        let target = self.low_pass_gpu.mapped(0);
        self.low_pass
            .write_to_pointer(read_index, write_index, target);

        let target = self.high_pass_gpu.mapped(0);
        self.high_pass
            .write_to_pointer(read_index, write_index, target);

        self.timer.section("Filters to GPU");

        run_dft(&self.audio.left, &mut self.signal_dft, &self.signal_dft_gpu);

        run_dft(
            &self.low_pass,
            &mut self.low_pass_dft,
            &self.low_pass_dft_gpu,
        );

        run_dft(
            &self.high_pass,
            &mut self.high_pass_dft,
            &self.high_pass_dft_gpu,
        );

        self.timer.section("DFTs and DFTs to GPU");

        let result = match unsafe { self.vulkan.tick() } {
            Err(err) => {
                error!("{err}");
                winit::event_loop::ControlFlow::ExitWithCode(1)
            }
            Ok(None) => winit::event_loop::ControlFlow::Poll,
            Ok(Some(vulkan::Event::Resized)) => {
                self.images.clear();
                match initialize_images(&mut self.vulkan) {
                    Ok(images) => {
                        self.images = images;
                        winit::event_loop::ControlFlow::Poll
                    }
                    Err(err) => {
                        error!("{err}");
                        winit::event_loop::ControlFlow::ExitWithCode(1)
                    }
                }
            }
        };

        self.timer.section("Vulkan");

        if self.vulkan.num_frames % 600 == 0 {
            self.timer.print();
        }

        result
    }
}

fn initialize_images(
    vulkan: &mut vulkan::Vulkan,
) -> Result<Vec<Rc<vulkan::multi_image::MultiImage>>, error::Error> {
    let image_size = vulkan.surface_info.surface_resolution;

    let mut images = Vec::new();

    let intermediate = vulkan.new_multi_image("intermediate", image_size, None)?;
    let intermediate_prev = vulkan.prev_shift(&intermediate, "intermediate_prev");
    images.push(intermediate);
    images.push(intermediate_prev);

    let highlights = vulkan.new_multi_image("highlights", image_size, None)?;
    images.push(highlights);
    let bloom_h = vulkan.new_multi_image("bloom_h", image_size, None)?;
    images.push(bloom_h);
    let bloom_hv = vulkan.new_multi_image("bloom_hv", image_size, None)?;
    images.push(bloom_hv);
    let result = vulkan.new_multi_image("result", image_size, None)?;
    let result_prev = vulkan.prev_shift(&result, "result_prev");
    images.push(result);
    images.push(result_prev);

    Ok(images)
}

impl Drop for Visualizer {
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

    /// Enable vsync
    #[arg(short, long, default_value = "true")]
    vsync: bool,
}

fn run_main() -> Result<(), Error> {
    let args = Args::parse();

    let initial_size = vk::Extent2D {
        width: 1280,
        height: 1024,
    };
    let mut window = window::Window::new(initial_size)?;
    let mut vulkan = vulkan::Vulkan::new(&window, &args.shader_paths, args.vsync)?;

    let images = initialize_images(&mut vulkan)?;

    let sample_rate = 44100;
    let audio_buffer_size = sample_rate * args.audio_buffer_sec;
    let audio_buffer_bytes = (audio_buffer_size * mem::size_of::<f32>()) as u64;

    let audio = audio::Audio::new(audio_buffer_size)?;
    let signal_gpu = vulkan.new_multi_buffer("signal", audio_buffer_bytes, Some(1))?;

    let low_pass = audio::low_pass::LowPass::new(audio_buffer_size, 0.02);
    let low_pass_gpu = vulkan.new_multi_buffer("low_pass", audio_buffer_bytes, Some(1))?;

    let high_pass = audio::high_pass::HighPass::new(audio_buffer_size, 0.1);
    let high_pass_gpu = vulkan.new_multi_buffer("high_pass", audio_buffer_bytes, Some(1))?;

    let dft_size = args.dft_size as f32;
    let dft_window_per_s = audio.sample_rate as f32 / dft_size;
    let dft_min_fq = dft_window_per_s * 1f32;
    let dft_max_fq = dft_window_per_s * dft_size / 2f32;
    info!("DFT can analyze frequencies in the range: {dft_min_fq} hz - {dft_max_fq} hz");

    let dft_result_size = dft::Dft::output_byte_size(args.dft_size) as u64;

    let signal_dft = dft::Dft::new(args.dft_size);
    let signal_dft_gpu = vulkan.new_multi_buffer("signal_dft", dft_result_size, Some(1))?;

    let low_pass_dft = dft::Dft::new(args.dft_size);
    let low_pass_dft_gpu = vulkan.new_multi_buffer("low_pass_dft", dft_result_size, Some(1))?;

    let high_pass_dft = dft::Dft::new(args.dft_size);
    let high_pass_dft_gpu = vulkan.new_multi_buffer("high_pass_dft", dft_result_size, Some(1))?;

    log::info!("Running...");
    {
        let mut visualizer = Visualizer {
            timer: Timer::new(0.999),
            available_samples: 0,
            avg_available_samples: 44100f32 / 60f32,
            avg_available_samples_alpha: 0.95,
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
            images,
            vulkan,
        };
        window.run_main_loop(&mut visualizer);
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
