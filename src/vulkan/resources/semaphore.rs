use std::{ops::Deref, rc::Rc};

use tracing::debug;

use ash::vk;

use crate::error::Error;

use super::device::Device;

pub struct Semaphore {
    device: Rc<Device>,
    semaphore: vk::Semaphore,
}

impl Deref for Semaphore {
    type Target = vk::Semaphore;

    fn deref(&self) -> &Self::Target {
        &self.semaphore
    }
}

impl Semaphore {
    pub unsafe fn new(device: &Rc<Device>) -> Result<Rc<Self>, Error> {
        debug!("Creating semaphore");
        let device = device.clone();
        let semaphore = device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;
        Ok(Rc::new(Semaphore { device, semaphore }))
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        debug!("Destroying semaphore");
        unsafe {
            self.device.destroy_semaphore(**self, None);
        }
    }
}
