// mod audio;
mod error;
mod vulkan;
mod window;

use log;
use simple_logger;

fn main() {
    simple_logger::init_with_level(log::Level::Debug).unwrap();
    log::info!("Initializing");

    let mut window = window::Window::new(1280, 1024).expect("Failed to open window");
    let mut vulkan = vulkan::Vulkan::new(&window).expect("Failed to initialize vulkan");

    let mut main_loop = || {
        vulkan.recompile_shader_if_modified();
        vulkan.render_next_frame();
        // vulkan.num_frames < 100
        true
    };

    window.run_main_loop(&mut main_loop);
    vulkan.wait_idle();

    // let audio = init_audio();

    // let mut x = 0;
}
