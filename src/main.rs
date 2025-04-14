use clap::Parser;

mod analysis;
mod audio;
mod cell;
mod error;
mod filters;
mod ring_buffer;
mod thread_shared;
mod utils;
mod visualizer;
mod vulkan;
mod window;

// Required to use run_return on event loop.
use winit::{
    event::VirtualKeyCode, event_loop::ControlFlow, platform::run_return::EventLoopExtRunReturn,
};

/// Run an audio visualizer.
#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// The shader module path
    #[arg(short, long, num_args = 0.., default_values = &["shaders/paint.comp", "shaders/present.comp"])]
    shader_paths: Vec<std::path::PathBuf>,

    /// The DFT size
    #[arg(short, long, default_value = "2048")]
    dft_size: usize,

    /// The audio buffer size
    #[arg(short, long, default_value = "5")]
    audio_buffer_sec: f32,

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

    #[arg(long, default_value = "110")]
    slowest_bpm: u32,
    #[arg(long, default_value = "160")]
    fastest_bpm: u32,
}

fn run_main(args: &Args) -> error::VResult<()> {
    // Audio launches its own pulseaudio something threads, no ticking required.
    let audio = audio::Audio::new(args.audio_buffer_sec, !args.no_virtual_sink)?;

    // The websocket server launches a tokio runtime and listens to a channel.
    // No ticking apart from populating the channel is required.
    let server = args.websocket.then(|| analysis::server::Server::start());

    // Analysis should be ticked once per "frame".
    let analysis = {
        let sender = server.as_ref().map(|(_, sender)| sender.clone());
        let sample_rate = audio.sample_rate() as f32;
        let analysis = analysis::Analysis::new(args, sample_rate, sender);
        cell::Cell::new(analysis)
    };

    // Notice Ctrl+C.
    let run = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    ctrlc::set_handler({
        let run = run.clone();
        move || {
            run.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    })
    .expect("Error setting Ctrl-C handler");

    // Choose the mainloop.
    if args.headless {
        // Use a custom headless mainloop.
        while run.load(std::sync::atomic::Ordering::SeqCst) {
            analysis.as_mut_ref().on_tick(&audio.signal);
            utils::sleep_ms(16);
        }
    } else {
        // The visualizer should be ticked once per frame.
        let (mut event_loop, visualizer) =
            visualizer::Visualizer::new(&args, &audio.signal, &analysis.as_ref())?;
        let visualizer = cell::Cell::new(visualizer);

        // Use the visual winit-based mainloop.
        event_loop.run_return(|event, &_, control_flow| {
            if !run.load(std::sync::atomic::Ordering::SeqCst) {
                *control_flow = ControlFlow::ExitWithCode(1)
            }

            *control_flow = match window::translate_event(event) {
                // No other events, run analysis and render a frame.
                window::Event::Tick => {
                    analysis.as_mut_ref().on_tick(&audio.signal);
                    match visualizer
                        .as_mut_ref()
                        .tick(&audio.signal, &analysis.as_ref())
                    {
                        Ok(()) => ControlFlow::Poll,
                        Err(err) => {
                            tracing::error!("Running vulkan tick failed: {err}");
                            ControlFlow::ExitWithCode(2)
                        }
                    }
                }

                // Obvious. Close requested via Alt+F4/Ctrl+Shift+Q.
                window::Event::Close => ControlFlow::ExitWithCode(0),

                window::Event::KeyPress(VirtualKeyCode::Escape | VirtualKeyCode::Q) => {
                    ControlFlow::ExitWithCode(0)
                }

                window::Event::KeyPress(VirtualKeyCode::R) => {
                    analysis.as_mut_ref().quarter_beat_index = 0;
                    ControlFlow::Poll
                }

                // Resize events can originate from both winit and vulkan.... Register the resize
                // event and wait until no resize events were recieved for X seconds.
                window::Event::Resize(width, height) => {
                    visualizer.as_mut_ref().debounce_resize(width, height);
                    ControlFlow::Poll
                }

                // Other events we don't care about.
                _ => ControlFlow::Poll,
            };
        });
    }

    Ok(())
}

struct CustomTime;

impl tracing_subscriber::fmt::time::FormatTime for CustomTime {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", chrono::Local::now().format("%H:%M:%S%.3f"))
    }
}

fn main() {
    // Set up a tracing subscriber.
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_timer(CustomTime)
        .with_max_level(tracing::Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set global subscriber");

    tracing::info!("Starting visualize-rs...");
    let args = Args::parse();
    if let Err(err) = run_main(&args) {
        tracing::error!("{}", err);
    }
    tracing::info!("Stopping visualize-rs...");
}
