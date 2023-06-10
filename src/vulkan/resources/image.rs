use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::{
    device::Device, surface_info::SurfaceInfo, swapchain::Swapchain,
    swapchain_loader::SwapchainLoader,
};

struct RegularImage {
    device: Rc<Device>,
    image: vk::Image,
}

struct SwapchainImage {
    image: vk::Image,
}

pub enum Image {
    Regular(RegularImage),
    Swapchain(SwapchainImage),
}

impl Deref for Image {
    type Target = vk::Image;

    fn deref(&self) -> &Self::Target {
        match self {
            Image::Regular(RegularImage { image, .. }) => &image,
            Image::Swapchain(SwapchainImage { image }) => &image,
        }
    }
}

impl Image {
    pub unsafe fn new(device: &Rc<Device>, surface_info: &SurfaceInfo) -> Result<Rc<Self>, Error> {
        debug!("Creating image");
        let device = device.clone();
        let image_create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .format(surface_info.surface_format.format)
            .extent(surface_info.surface_resolution.into())
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::STORAGE)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = device.create_image(&image_create_info, None)?;
        let image = Image::Regular(RegularImage { device, image });
        Ok(Rc::new(image))
    }

    pub unsafe fn many_from_swapchain(
        swapchain_loader: &SwapchainLoader,
        swapchain: &Swapchain,
    ) -> Result<Vec<Rc<Self>>, Error> {
        debug!("Creating images");
        let images = swapchain_loader
            .get_swapchain_images(**swapchain)?
            .iter()
            .map(|&image| Rc::new(Image::Swapchain(SwapchainImage { image })))
            .collect();
        Ok(images)
    }

    pub unsafe fn get_required_memory_size(&self) -> Option<vk::DeviceSize> {
        match self {
            Image::Regular(RegularImage { device, image }) => {
                Some(device.get_image_memory_requirements(*image).size)
            }
            Image::Swapchain(..) => None,
        }
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        match self {
            Image::Regular(RegularImage { device, image }) => {
                debug!("Destroying image");
                unsafe {
                    device.destroy_image(*image, None);
                }
            }
            Image::Swapchain(..) => (),
        }
    }
}
