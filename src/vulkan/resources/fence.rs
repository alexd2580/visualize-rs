use std::{ops::Deref, rc::Rc};

use tracing::debug;

use ash::vk;

use crate::error::Error;

use super::device::Device;

pub struct Fence {
    device: Rc<Device>,
    fence: vk::Fence,
}

impl Deref for Fence {
    type Target = vk::Fence;

    fn deref(&self) -> &Self::Target {
        &self.fence
    }
}

impl Fence {
    pub unsafe fn new(device: &Rc<Device>) -> Result<Rc<Self>, Error> {
        debug!("Fence()");
        let device = device.clone();
        let create_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
        let fence = device.create_fence(&create_info, None)?;
        Ok(Rc::new(Fence { device, fence }))
    }

    pub unsafe fn wait(&self) -> Result<(), Error> {
        Ok(self
            .device
            .wait_for_fences(&[self.fence], true, std::u64::MAX)?)
    }

    pub unsafe fn reset(&self) -> Result<(), Error> {
        Ok(self.device.reset_fences(&[self.fence])?)
    }
}

impl Drop for Fence {
    fn drop(&mut self) {
        debug!("Destroying fence");
        unsafe {
            self.device.destroy_fence(**self, None);
        }
    }
}
