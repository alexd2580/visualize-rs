use std::{thread, time};

use log::debug;

use ash::vk::{self, SurfaceKHR as VkSurface};
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use winit::{
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    platform::run_return::EventLoopExtRunReturn,
    window::{Fullscreen, Window as WinitWindow, WindowBuilder},
};

use crate::error::Error;
use crate::vulkan::resources::instance::Instance;

pub trait App {
    fn loop_body(&mut self) -> ControlFlow;
}

pub struct Window {
    pub size: vk::Extent2D,
    event_loop: EventLoop<()>,
    window: WinitWindow,
}

impl Window {
    pub fn new(size: vk::Extent2D) -> Result<Self, Error> {
        debug!("Initializing video system");

        let event_loop = EventLoop::new();
        // let monitor = event_loop
        //     .available_monitors()
        //     .next()
        //     .ok_or_else(|| Error::Local("No monitors found?!".to_owned()))?;
        //
        // let mode = monitor
        //     .video_modes()
        //     .next()
        //     .ok_or_else(|| Error::Local("Monitor doesn't have any modes?!".to_owned()))?;

        let logical_size = winit::dpi::LogicalSize::new(size.width, size.height);
        let window = WindowBuilder::new()
            .with_title("visualize-rs")
            .with_inner_size(logical_size)
            .with_fullscreen(Some(Fullscreen::Borderless(None)))
            .build(&event_loop)?;

        Ok(Window {
            size,
            event_loop,
            window,
        })
    }

    pub fn enumerate_required_extensions(&self) -> Result<Vec<*const i8>, Error> {
        let raw_handle = self.window.raw_display_handle();
        let extensions = ash_window::enumerate_required_extensions(raw_handle)?;
        Ok(extensions.to_vec())
    }

    pub fn create_surface(
        &self,
        entry: &ash::Entry,
        instance: &Instance,
    ) -> Result<VkSurface, Error> {
        unsafe {
            Ok(ash_window::create_surface(
                entry,
                instance,
                self.window.raw_display_handle(),
                self.window.raw_window_handle(),
                None,
            )?)
        }
    }

    fn handle_event<T: App>(event: &Event<()>, app: &mut T) -> ControlFlow {
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
            Event::MainEventsCleared => app.loop_body(),
            _ => ControlFlow::Poll,
        }
    }

    pub fn run_main_loop<T: App>(&mut self, app: &mut T) {
        self.event_loop.run_return(|event, &_, control_flow| {
            *control_flow = Window::handle_event(&event, app);
        });
    }
}
