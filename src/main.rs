use std::{rc::Rc, time};

use cell::Cell;
use clap::Parser;

mod analysis;
mod audio;
mod averages;
mod beat_analysis;
mod cell;
mod dft;
mod error;
mod ring_buffer;
mod server;
mod thread_shared;
mod timer;
mod utils;
mod vulkan;
mod window;

// Required to use run_return on event loop.
use winit::platform::run_return::EventLoopExtRunReturn;

struct Visualizer {
    signal_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,
    signal_dft_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,

    low_pass_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,
    low_pass_dft_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,

    high_pass_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,
    high_pass_dft_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,

    // let dft_result_size = Dft::output_byte_size(args.dft_size) + mem::size_of::<i32>();
    // history: History::new(history_size),
    // history_gpu: vulkan.new_multi_buffer("history", history_gpu_size, Some(1))?,
    // // Averages.
    // long_avg: AlphaAvg::new(0.99),
    // short_avg: WindowedAvg::new((0.2 * frame_rate as f32) as usize),
    // // Beat detection.
    // noise_threshold_factor: 0.25,
    // beat_sigma_threshold_factor: 2.2,
    // is_high: false,
    // is_beat: false,
    // // BPM detection.
    // autocorrelation: Dft::new(8 * frame_rate),
    // autocorrelation_gpu: vulkan.new_multi_buffer(
    //     "autocorrelation",
    //     autocorrelation_gpu_size,
    //     Some(1),
    // )?,

    // These should be dropped last.
    images: Vec<Rc<vulkan::multi_image::MultiImage>>,
    vulkan: vulkan::Vulkan,
}

impl Visualizer {
    fn reinitialize_images(&mut self) -> error::VResult<()> {
        // Drop old images.
        self.images.clear();

        let vulkan = &mut self.vulkan;
        let image_size = vulkan.surface_info.surface_resolution;

        let intermediate = vulkan.new_multi_image("intermediate", image_size, None)?;
        let intermediate_prev = vulkan.prev_shift(&intermediate, "intermediate_prev");
        self.images.push(intermediate);
        self.images.push(intermediate_prev);

        let highlights = vulkan.new_multi_image("highlights", image_size, None)?;
        self.images.push(highlights);
        let bloom_h = vulkan.new_multi_image("bloom_h", image_size, None)?;
        self.images.push(bloom_h);
        let bloom_hv = vulkan.new_multi_image("bloom_hv", image_size, None)?;
        self.images.push(bloom_hv);
        let result = vulkan.new_multi_image("result", image_size, None)?;
        let result_prev = vulkan.prev_shift(&result, "result_prev");
        self.images.push(result);
        self.images.push(result_prev);

        Ok(())
    }

    fn new(
        args: &Args,
        analysis: &analysis::Analysis,
    ) -> error::VResult<(winit::event_loop::EventLoop<()>, Visualizer)> {
        let (event_loop, window) = window::Window::new()?;
        let mut vulkan = vulkan::Vulkan::new(&window, &args.shader_paths, !args.no_vsync)?;

        let signal_gpu =
            vulkan.new_multi_buffer("signal", analysis.audio.signal.serialized_size(), Some(1))?;
        let low_pass_gpu =
            vulkan.new_multi_buffer("low_pass", analysis.low_pass.serialized_size(), Some(1))?;
        let high_pass_gpu =
            vulkan.new_multi_buffer("high_pass", analysis.high_pass.serialized_size(), Some(1))?;

        let signal_dft_gpu = vulkan.new_multi_buffer(
            "signal_dft",
            analysis.signal_dft.serialized_size(),
            Some(1),
        )?;
        let low_pass_dft_gpu = vulkan.new_multi_buffer(
            "low_pass_dft",
            analysis.low_pass_dft.serialized_size(),
            Some(1),
        )?;
        let high_pass_dft_gpu = vulkan.new_multi_buffer(
            "high_pass_dft",
            analysis.high_pass_dft.serialized_size(),
            Some(1),
        )?;

        let mut visualizer = Self {
            signal_gpu,
            signal_dft_gpu,
            low_pass_gpu,
            low_pass_dft_gpu,
            high_pass_gpu,
            high_pass_dft_gpu,
            images: Vec::new(),
            vulkan,
        };

        visualizer.reinitialize_images()?;
        Ok((event_loop, visualizer))
    }

    fn run_vulkan(
        &mut self,
        push_constant_values: std::collections::HashMap<String, vulkan::Value>,
    ) -> error::VResult<()> {
        match unsafe { self.vulkan.tick(&push_constant_values)? } {
            None => (),
            Some(vulkan::Event::Resized) => self.reinitialize_images()?,
        }
        Ok(())
    }

    fn tick(&mut self, analysis: &analysis::Analysis) -> winit::event_loop::ControlFlow {
        use vulkan::Value::{Bool, F32};

        let read_index = analysis.read_index;
        let write_index = analysis.write_index;

        analysis
            .audio
            .signal
            .write_to_pointer(read_index, write_index, self.signal_gpu.mapped(0));
        analysis
            .low_pass
            .write_to_pointer(read_index, write_index, self.low_pass_gpu.mapped(0));
        analysis
            .high_pass
            .write_to_pointer(read_index, write_index, self.high_pass_gpu.mapped(0));

        analysis
            .signal_dft
            .write_to_pointer(self.signal_dft_gpu.mapped(0));
        analysis
            .low_pass_dft
            .write_to_pointer(self.low_pass_dft_gpu.mapped(0));
        analysis
            .high_pass_dft
            .write_to_pointer(self.high_pass_dft_gpu.mapped(0));

        let mut push_constant_values = std::collections::HashMap::new();

        let is_beat = analysis.beat_analysis.is_beat;
        push_constant_values.insert("is_beat".to_owned(), Bool(is_beat));
        let now = analysis.epoch.elapsed().as_secs_f32();
        push_constant_values.insert("now".to_owned(), F32(now));

        let result = match self.run_vulkan(push_constant_values) {
            Ok(()) => winit::event_loop::ControlFlow::Poll,
            Err(err) => {
                log::error!("{err}");
                winit::event_loop::ControlFlow::ExitWithCode(1)
            }
        };

        self.vulkan.num_frames += 1;

        result
    }
}

impl Drop for Visualizer {
    fn drop(&mut self) {
        self.vulkan.wait_idle();
    }
}

/// Run an audio visualizer.
#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// The shader module path
    #[arg(short, long, num_args = 0.., default_value = "shaders/debug.comp")]
    shader_paths: Vec<std::path::PathBuf>,

    /// The DFT size
    #[arg(short, long, default_value = "2048")]
    dft_size: usize,

    /// The audio buffer size
    #[arg(short, long, default_value = "4")]
    audio_buffer_sec: u32,

    /// Enable vsync
    #[arg(long, action = clap::ArgAction::SetTrue)]
    no_vsync: bool,

    /// Redirect the audio through a virtual pulseaudio sink
    #[arg(long, action = clap::ArgAction::SetTrue)]
    no_virtual_sink: bool,

    /// Create a websocket server that echoes some info
    #[arg(long, action = clap::ArgAction::SetTrue)]
    websocket: bool,

    /// Display the visualizer
    #[arg(long, action = clap::ArgAction::SetTrue)]
    headless: bool,
}

fn run_main(args: &Args) -> error::VResult<()> {
    // Audio launches its own pulseaudio something threads, no ticking required.
    let audio = audio::Audio::new(args.audio_buffer_sec, !args.no_virtual_sink)?;

    // The websocket server launches a tokio runtime and listens to a channel.
    // No ticking apart from populating the channel is required.
    let server = args.websocket.then(|| server::Server::start());

    // Analysis should be ticked once per "frame".
    let analysis = {
        let sender = server.as_ref().map(|(_, sender)| sender.clone());
        let analysis = analysis::Analysis::new(args, audio, sender);
        Cell::new(analysis)
    };

    // The visualizer should also be ticked once per frame.
    let visualizer = (!args.headless)
        .then(|| Visualizer::new(&args, &analysis.as_ref()))
        .transpose()?;

    // Choose the mainloop.
    if let Some((mut event_loop, visualizer)) = visualizer {
        // Use the visual winit-based mainloop.
        let visualizer = Cell::new(visualizer);
        event_loop.run_return(|event, &_, control_flow| {
            *control_flow = window::handle_event(&event, &|| {
                analysis.as_mut_ref().tick();
                visualizer.as_mut_ref().tick(&analysis.as_ref())
            });
        });
    } else {
        // Use a custom headless one.
        let run = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        ctrlc::set_handler({
            let run = run.clone();
            move || {
                run.store(false, std::sync::atomic::Ordering::SeqCst);
            }
        })
        .expect("Error setting Ctrl-C handler");
        while run.load(std::sync::atomic::Ordering::SeqCst) {
            analysis.as_mut_ref().tick();
            std::thread::sleep(time::Duration::from_millis(16));
        }
    }

    Ok(())
}

fn main() {
    simple_logger::init_with_level(log::Level::Debug).unwrap();
    log::info!("Initializing...");
    let args = Args::parse();
    if let Err(err) = run_main(&args) {
        log::error!("{}", err);
    }
    log::info!("Terminating...");
}
