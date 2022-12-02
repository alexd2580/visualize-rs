use std::{ops::Deref, rc::Rc};

use ash::{self, vk};
use log::debug;

use crate::error::Error;

use super::device::Device;

pub struct Fence {
    device: Rc<Device>,
    fence: vk::Fence,
}

impl Fence {
    pub fn new(device: Rc<Device>) -> Result<Self, Error> {
        let create_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
        let fence = unsafe { device.create_fence(&create_info, None) }.map_err(Error::VkError)?;
        Ok(Fence { device, fence })
    }

    pub fn wait(&self) {
        unsafe {
            self.device
                .wait_for_fences(&[self.fence], true, std::u64::MAX)
        }
        .expect("Failed to wait for fence.");
    }

    pub fn reset(&self) {
        unsafe { self.device.reset_fences(&[self.fence]) }.expect("Failed to reset fence.");
    }
}

impl Deref for Fence {
    type Target = vk::Fence;

    fn deref(&self) -> &Self::Target {
        &self.fence
    }
}

impl Drop for Fence {
    fn drop(&mut self) {
        debug!("Destroying fence");
        unsafe {
            self.device.destroy_fence(self.fence, None);
        }
    }
}
