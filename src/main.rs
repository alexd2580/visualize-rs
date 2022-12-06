mod audio;
mod audio_buffer;
mod error;
mod vulkan;
mod window;

fn main() {
    simple_logger::init_with_level(log::Level::Debug).unwrap();
    log::info!("Initializing");

    let audio_buffer = audio_buffer::AudioBuffer::new();
    let _audio = audio::Audio::new(&audio_buffer);

    let mut window = window::Window::new(1280, 1024).expect("Failed to open window");
    let mut vulkan = vulkan::Vulkan::new(&window).expect("Failed to initialize vulkan");

    log::info!("Running");

    window.run_main_loop(&mut vulkan);
    vulkan.wait_idle();

    log::info!("Terminating");
}
