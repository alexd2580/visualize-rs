use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::{error::Error, window::Window};

use super::{entry::Entry, instance::Instance, surface_loader::SurfaceLoader};

pub struct Surface {
    surface_loader: Rc<SurfaceLoader>,
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
        entry: &Entry,
        instance: &Instance,
        surface_loader: &Rc<SurfaceLoader>,
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
