use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::{swapchain::Swapchain, swapchain_loader::SwapchainLoader};

pub struct Images {
    images: Vec<vk::Image>,
}

impl Deref for Images {
    type Target = [vk::Image];

    fn deref(&self) -> &Self::Target {
        &self.images
    }
}

impl Images {
    pub unsafe fn new(
        swapchain_loader: &SwapchainLoader,
        swapchain: &Swapchain,
    ) -> Result<Rc<Self>, Error> {
        debug!("Creating images");
        let images = swapchain_loader.get_swapchain_images(**swapchain)?;
        Ok(Rc::new(Images { images }))
    }
}
