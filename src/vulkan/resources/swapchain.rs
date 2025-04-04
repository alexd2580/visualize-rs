use std::{ops::Deref, rc::Rc};

use ash::{extensions::khr::Swapchain as SwapchainLoader, vk};

use tracing::debug;

use crate::error::Error;

use super::{surface::Surface, surface_info::SurfaceInfo};

pub struct Swapchain {
    swapchain_loader: SwapchainLoader,
    swapchain: vk::SwapchainKHR,
}

impl Deref for Swapchain {
    type Target = vk::SwapchainKHR;

    fn deref(&self) -> &Self::Target {
        &self.swapchain
    }
}

impl Swapchain {
    pub unsafe fn new(
        surface: &Surface,
        surface_info: &SurfaceInfo,
        swapchain_loader: &SwapchainLoader,
        old_swapchain: Option<vk::SwapchainKHR>,
    ) -> Result<Rc<Swapchain>, Error> {
        debug!("Creating swapchain");
        let swapchain_loader = swapchain_loader.clone();
        let surface_format = &surface_info.surface_format;

        let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(**surface)
            .min_image_count(u32::try_from(surface_info.desired_image_count).unwrap())
            .image_color_space(surface_format.color_space)
            .image_format(surface_format.format)
            .image_extent(surface_info.surface_resolution)
            .image_usage(
                vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED,
            )
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(vk::SurfaceTransformFlagsKHR::IDENTITY)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(surface_info.desired_present_mode)
            .clipped(true)
            .image_array_layers(1)
            .old_swapchain(old_swapchain.unwrap_or(vk::SwapchainKHR::null()));

        let swapchain = swapchain_loader.create_swapchain(&swapchain_create_info, None)?;

        Ok(Rc::new(Swapchain {
            swapchain_loader,
            swapchain,
        }))
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        debug!("Destroying swapchain");
        unsafe { self.swapchain_loader.destroy_swapchain(**self, None) };
    }
}
