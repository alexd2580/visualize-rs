use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::extensions;

use crate::error::Error;

use super::{device::Device, instance::Instance};

pub struct SwapchainLoader {
    swapchain_loader: extensions::khr::Swapchain,
}

impl Deref for SwapchainLoader {
    type Target = extensions::khr::Swapchain;

    fn deref(&self) -> &Self::Target {
        &self.swapchain_loader
    }
}

impl SwapchainLoader {
    pub fn new(instance: &Instance, device: &Device) -> Result<Rc<SwapchainLoader>, Error> {
        debug!("Creating swapchain loader");
        let swapchain_loader = extensions::khr::Swapchain::new(instance, device);

        Ok(Rc::new(SwapchainLoader { swapchain_loader }))
    }
}
