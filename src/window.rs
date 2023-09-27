use std::{thread, time};

use log::debug;

use ash::vk::SurfaceKHR as VkSurface;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use winit::{
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window as WinitWindow, WindowBuilder},
};

use crate::{error::VResult, vulkan::resources::instance::Instance};

pub struct Window(WinitWindow);

impl Window {
    pub fn new() -> VResult<(EventLoop<()>, Self)> {
        debug!("Initializing video system");

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

pub fn handle_event(event: &Event<()>, tick: &dyn Fn() -> ControlFlow) -> ControlFlow {
    match event {
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => ControlFlow::Exit,
        Event::WindowEvent {
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
        } => match key {
            VirtualKeyCode::Escape | VirtualKeyCode::Q => ControlFlow::Exit,
            VirtualKeyCode::K => {
                thread::sleep(time::Duration::from_secs(1));
                ControlFlow::Poll
            }
            _ => ControlFlow::Poll,
        },
        Event::WindowEvent {
            event: WindowEvent::Resized(..),
            ..
        } => {
            // Ignoring window event. Resize handled via Vulkan.
            ControlFlow::Poll
        }
        Event::MainEventsCleared => tick(),
        _ => ControlFlow::Poll,
    }
}
