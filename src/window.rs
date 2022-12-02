extern crate ash;
extern crate ash_window;
extern crate libpulse_binding as pulse;
extern crate raw_window_handle;
extern crate winit;

use crate::vulkan::instance::Instance;
use ash::vk::SurfaceKHR as VkSurface;
use log::debug;

use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use winit::{
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    platform::run_return::EventLoopExtRunReturn,
    window::{Window as WinitWindow, WindowBuilder},
};

use crate::error::Error;

pub struct Window {
    pub width: u32,
    pub height: u32,
    event_loop: EventLoop<()>,
    window: WinitWindow,
}

impl Window {
    pub fn new(width: u32, height: u32) -> Result<Self, Error> {
        debug!("Initializing video system");

        let event_loop = EventLoop::new();
        let size = winit::dpi::LogicalSize::new(width, height);
        let window_or_err = WindowBuilder::new()
            .with_title("visualize-rs")
            .with_inner_size(size)
            .build(&event_loop);
        let window = window_or_err.map_err(Error::OsError)?;

        Ok(Window {
            width,
            height,
            event_loop,
            window,
        })
    }

    pub fn enumerate_required_extensions(&self) -> Result<Vec<*const i8>, Error> {
        let raw_handle = self.window.raw_display_handle();
        let extensions_vk = ash_window::enumerate_required_extensions(raw_handle);
        let extensions = extensions_vk.map_err(Error::VkError)?;
        Ok(extensions.to_vec())
    }

    pub fn create_surface(&self, instance: &Instance) -> Result<VkSurface, Error> {
        unsafe {
            ash_window::create_surface(
                &instance.entry,
                &instance.instance,
                self.window.raw_display_handle(),
                self.window.raw_window_handle(),
                None,
            )
        }
        .map_err(Error::VkError)
    }

    fn handle_event(
        event: Event<()>,
        control_flow: &mut ControlFlow,
        run_logic: &mut dyn FnMut() -> bool,
    ) {
        *control_flow = ControlFlow::Poll;
        match event {
            Event::WindowEvent {
                event:
                    WindowEvent::CloseRequested
                    | WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                state: ElementState::Pressed,
                                virtual_keycode: Some(VirtualKeyCode::Escape),
                                ..
                            },
                        ..
                    },
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::MainEventsCleared => {
                if !run_logic() {
                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => (),
        }
    }

    pub fn run_main_loop(&mut self, run_logic: &mut dyn FnMut() -> bool) {
        self.event_loop.run_return(|event, &_, control_flow| {
            Window::handle_event(event, control_flow, run_logic)
        });
    }
}
