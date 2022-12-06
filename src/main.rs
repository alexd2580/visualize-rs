use std::sync::Arc;

mod audio;
mod audio_buffer;
mod error;
mod vulkan;
mod window;

struct App<'a, 'b> {
    audio_buffer: &'a Arc<audio_buffer::AudioBuffer>,
    vulkan: &'b mut vulkan::Vulkan,
}

impl<'a, 'b> App<'a, 'b> {
    fn new(
        audio_buffer: &'a Arc<audio_buffer::AudioBuffer>,
        vulkan: &'b mut vulkan::Vulkan,
    ) -> Self {
        App {
            audio_buffer,
            vulkan,
        }
    }
}

impl<'a, 'b> window::App for App<'a, 'b> {
    fn run_frame(&mut self) -> winit::event_loop::ControlFlow {
        self.vulkan.run_frame()
    }

    fn handle_resize(&mut self, new_size: (u32, u32)) -> Result<(), error::Error> {
        self.vulkan.handle_resize(new_size)
    }
}

fn main() {
    simple_logger::init_with_level(log::Level::Debug).unwrap();
    log::info!("Initializing");

    let audio_buffer = audio_buffer::AudioBuffer::new();
    let _audio = audio::Audio::new(&audio_buffer);

    let mut window = window::Window::new(1280, 1024).expect("Failed to open window");
    let mut vulkan = vulkan::Vulkan::new(&window).expect("Failed to initialize vulkan");

    log::info!("Running");
    {
        let mut app = App::new(&audio_buffer, &mut vulkan);
        window.run_main_loop(&mut app);
    }

    vulkan.wait_idle();

    log::info!("Terminating");
}
