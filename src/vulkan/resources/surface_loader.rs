use std::{ops::Deref, rc::Rc};

use log::debug;

use ash;
use ash::extensions;

use crate::error::Error;

use super::entry::Entry;
use super::instance::Instance;

pub struct SurfaceLoader {
    surface_loader: extensions::khr::Surface,
}

impl Deref for SurfaceLoader {
    type Target = extensions::khr::Surface;

    fn deref(&self) -> &Self::Target {
        &self.surface_loader
    }
}

impl SurfaceLoader {
    pub fn new(entry: &Entry, instance: &Instance) -> Result<Rc<Self>, Error> {
        debug!("Creating surface loader");
        let surface_loader = extensions::khr::Surface::new(entry, instance);
        Ok(Rc::new(SurfaceLoader { surface_loader }))
    }
}
