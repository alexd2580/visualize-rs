use std::ops::Deref;

use ash::vk;

use crate::error::Error;
use log::debug;

pub struct ImageSubresourceRange {
    image_subresource_range: vk::ImageSubresourceRange,
}

impl Deref for ImageSubresourceRange {
    type Target = vk::ImageSubresourceRange;

    fn deref(&self) -> &Self::Target {
        &self.image_subresource_range
    }
}

impl ImageSubresourceRange {
    pub fn new() -> Result<ImageSubresourceRange, Error> {
        debug!("Creating image subresource range");
        let image_subresource_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };

        Ok(ImageSubresourceRange {
            image_subresource_range,
        })
    }
}
