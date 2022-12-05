use crate::vulkan::instance::Instance;
use ash::vk::SurfaceKHR as VkSurface;
use log::{debug, error};

use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    platform::run_return::EventLoopExtRunReturn,
    window::{Window as WinitWindow, WindowBuilder},
};

use crate::error::Error;

pub trait App {
    fn run_frame(&mut self) -> ControlFlow;
    fn handle_resize(&mut self, new_size: (u32, u32)) -> Result<(), Error>;
}

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
        let window = WindowBuilder::new()
            .with_title("visualize-rs")
            .with_inner_size(size)
            .build(&event_loop)
            .map_err(Error::Os)?;

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
        let extensions = extensions_vk.map_err(Error::Vk)?;
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
        .map_err(Error::Vk)
    }

    fn handle_event<T: App>(event: Event<()>, app: &mut T) -> ControlFlow {
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
            } => ControlFlow::Exit,
            Event::WindowEvent {
                event: WindowEvent::Resized(PhysicalSize { width, height }),
                ..
            } => match app.handle_resize((width, height)) {
                Ok(()) => ControlFlow::Poll,
                Err(error) => {
                    error!("Failed to handle window resize: {:?}", error);
                    ControlFlow::ExitWithCode(1)
                }
            },
            Event::MainEventsCleared => app.run_frame(),
            _ => ControlFlow::Poll,
        }
    }

    pub fn run_main_loop<T: App>(&mut self, app: &mut T) {
        self.event_loop
            .run_return(|event, &_, control_flow| *control_flow = Window::handle_event(event, app));
    }
}
