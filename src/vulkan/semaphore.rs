use std::{ops::Deref, rc::Rc};

use ash::{self, vk};
use log::debug;

use crate::error::Error;

use super::device::Device;

pub struct Semaphore {
    device: Rc<Device>,
    semaphore: vk::Semaphore,
}

impl Semaphore {
    pub fn new(device: Rc<Device>) -> Result<Self, Error> {
        let semaphore =
            unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) }
                .map_err(Error::VkError)?;
        Ok(Semaphore { device, semaphore })
    }
}

impl Deref for Semaphore {
    type Target = vk::Semaphore;

    fn deref(&self) -> &Self::Target {
        &self.semaphore
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        debug!("Destroying semaphore");
        unsafe {
            self.device.destroy_semaphore(self.semaphore, None);
        }
    }
}
