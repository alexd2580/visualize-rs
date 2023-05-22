use std::mem;

use realfft::num_complex::Complex;

mod audio;
mod dft;
mod error;
mod vulkan;
mod window;

struct App {
    audio: audio::Audio,
    dft: dft::Dft,
    dft_buffer: vulkan::buffer::Buffer,
    vulkan: vulkan::Vulkan,
}

impl window::App for App {
    fn run_frame(&mut self) -> winit::event_loop::ControlFlow {
        self.audio.write_to_buffer(self.dft.get_input_vec());
        self.dft.run_transform();
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

fn main() {
    simple_logger::init_with_level(log::Level::Debug).unwrap();
    log::info!("Initializing");

    let mut window = window::Window::new(1280, 1024).expect("Failed to open window");
    let vulkan = vulkan::Vulkan::new(&window).expect("Failed to initialize vulkan");

    let audio = audio::Audio::new();
    let dft = dft::Dft::new();
    let dft_result_size = (dft.get_output_vec().len() * mem::size_of::<Complex<f32>>()) as u64;
    let dft_buffer = vulkan::buffer::Buffer::new(&vulkan.device, 123, dft_result_size, 3)
        .expect("Failed to allocate DFT buffer");

    log::info!("Running");
    {
        let mut app = App {
            audio,
            dft,
            dft_buffer,
            vulkan,
        };
        window.run_main_loop(&mut app);
    }

    log::info!("Terminating");
}
