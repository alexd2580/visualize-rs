use crate::{vulkan::instance::Instance, window::Window};
use ash::{extensions, vk};
use log::debug;

use crate::error::Error;

use super::physical_device::PhysicalDevice;

pub struct Surface {
    pub surface_loader: extensions::khr::Surface,
    pub surface: vk::SurfaceKHR,
}

fn all<T1, T2, T3, E>(
    (t1, t2, t3): (Result<T1, E>, Result<T2, E>, Result<T3, E>),
) -> Result<(T1, T2, T3), E> {
    Ok((t1?, t2?, t3?))
}

impl Surface {
    pub fn new(instance: &Instance, window: &Window) -> Result<Self, Error> {
        debug!("Initializing surface");

        let surface_loader = extensions::khr::Surface::new(&instance.entry, &instance.instance);
        let surface = window.create_surface(instance)?;
        Ok(Surface {
            surface_loader,
            surface,
        })
    }

    pub fn get_formats_capabilities_present_modes(
        &self,
        physical_device: &PhysicalDevice,
    ) -> Result<
        (
            Vec<ash::vk::SurfaceFormatKHR>,
            ash::vk::SurfaceCapabilitiesKHR,
            Vec<ash::vk::PresentModeKHR>,
        ),
        Error,
    > {
        let loader = &self.surface_loader;
        let surface = self.surface;
        let physical_device = physical_device.physical_device;
        unsafe {
            all((
                loader.get_physical_device_surface_formats(physical_device, surface),
                loader.get_physical_device_surface_capabilities(physical_device, surface),
                loader.get_physical_device_surface_present_modes(physical_device, surface),
            ))
            .map_err(Error::Vk)
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { self.surface_loader.destroy_surface(self.surface, None) };
    }
}
