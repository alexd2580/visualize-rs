use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::{extensions::khr::Surface as SurfaceLoader, vk};

use crate::{error::Error, window::Window};

use super::instance::Instance;

pub struct Surface {
    surface_loader: SurfaceLoader,
    surface: vk::SurfaceKHR,
}

impl Deref for Surface {
    type Target = vk::SurfaceKHR;

    fn deref(&self) -> &Self::Target {
        &self.surface
    }
}

impl Surface {
    pub fn new(
        window: &Window,
        entry: &ash::Entry,
        instance: &Instance,
        surface_loader: &SurfaceLoader,
    ) -> Result<Rc<Self>, Error> {
        debug!("Creating surface");
        let surface_loader = surface_loader.clone();

        let surface = window.create_surface(entry, instance)?;
        Ok(Rc::new(Surface {
            surface_loader,
            surface,
        }))
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        debug!("Destroying surface");
        unsafe {
            self.surface_loader.destroy_surface(**self, None);
        }
    }
}
