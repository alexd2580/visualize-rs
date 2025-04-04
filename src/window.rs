use tracing::{debug, instrument};

use ash::vk::SurfaceKHR as VkSurface;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use winit::{
    dpi::PhysicalSize,
    event::{self, ElementState, KeyboardInput, WindowEvent},
    event_loop::EventLoop,
    window::{Window as WinitWindow, WindowBuilder},
};

use crate::{error::VResult, vulkan::resources::instance::Instance};

pub struct Window(WinitWindow);

// impl Debug for Window {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(f, "Window")
//     }
// }

impl Window {
    #[instrument]
    pub fn new() -> VResult<(EventLoop<()>, Self)> {
        let event_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_title("visualize-rs")
            .build(&event_loop)?;

        Ok((event_loop, Window(window)))
    }

    pub fn enumerate_required_extensions(&self) -> VResult<Vec<*const i8>> {
        let raw_handle = self.0.raw_display_handle();
        let extensions = ash_window::enumerate_required_extensions(raw_handle)?;
        Ok(extensions.to_vec())
    }

    pub fn create_surface(&self, entry: &ash::Entry, instance: &Instance) -> VResult<VkSurface> {
        unsafe {
            Ok(ash_window::create_surface(
                entry,
                instance,
                self.0.raw_display_handle(),
                self.0.raw_window_handle(),
                None,
            )?)
        }
    }
}

pub enum Event {
    Tick,
    Close,
    KeyPress(event::VirtualKeyCode),
    Resize(u32, u32),
    Other,
}

pub fn translate_event(event: event::Event<()>) -> Event {
    match event {
        event::Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => Event::Close,
        event::Event::WindowEvent {
            event:
                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            state: ElementState::Pressed,
                            virtual_keycode: Some(key),
                            ..
                        },
                    ..
                },
            ..
        } => Event::KeyPress(key),

        // match key {
        //     VirtualKeyCode::Escape | VirtualKeyCode::Q => ControlFlow::Exit,
        //     VirtualKeyCode::K => {
        //         thread::sleep(time::Duration::from_secs(1));
        //         ControlFlow::Poll
        //     }
        //     _ => ControlFlow::Poll,
        // },
        event::Event::WindowEvent {
            event: WindowEvent::Resized(PhysicalSize { width, height }),
            ..
        } => Event::Resize(width, height),
        event::Event::MainEventsCleared => Event::Tick,
        _ => Event::Other,
    }
}
