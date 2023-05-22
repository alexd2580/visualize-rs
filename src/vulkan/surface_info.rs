use crate::error::Error;
use ash::vk;
use log::debug;

use super::{physical_device::PhysicalDevice, surface::Surface};

#[derive(Debug)]
pub struct SurfaceInfo {
    pub surface_format: vk::SurfaceFormatKHR,
    pub surface_capabilities: vk::SurfaceCapabilitiesKHR,
    pub desired_present_mode: vk::PresentModeKHR,
    pub desired_image_count: u32,
    pub surface_resolution: vk::Extent2D,
}

impl SurfaceInfo {
    pub fn new(
        (width, height): (u32, u32),
        physical_device: &PhysicalDevice,
        surface: &Surface,
    ) -> Result<Self, Error> {
        debug!("Collecting surface info");
        let (surface_formats, surface_capabilities, present_modes) =
            surface.get_formats_capabilities_present_modes(physical_device)?;
        let surface_format = surface_formats[0];

        // For reference see:
        // https://www.reddit.com/r/vulkan/comments/9txqqb/what_is_presentation_mode/
        let desired_present_mode = present_modes
            .into_iter()
            .find(|&mode| mode == vk::PresentModeKHR::FIFO)
            .ok_or_else(|| Error::Local("There is no vsync present mode".to_owned()))?;

        // Check that the surface supports storage write/can be used in compute shaders.
        if !surface_capabilities
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::STORAGE)
        {
            return Err(Error::Local(
                "Surface cannot be used for storage".to_owned(),
            ));
        }

        // Try to get triple buffering, fall back to double-buffering.
        // Assuming all modern GPUs support double buffering.
        let min_image_count = surface_capabilities.min_image_count;
        let max_image_count = surface_capabilities.max_image_count;
        let mut desired_image_count = min_image_count + 1;
        if max_image_count != 0 && desired_image_count > max_image_count {
            desired_image_count = max_image_count;
        }

        let surface_resolution = match surface_capabilities.current_extent.width {
            std::u32::MAX => vk::Extent2D { width, height },
            _ => surface_capabilities.current_extent,
        };

        Ok(SurfaceInfo {
            surface_format,
            surface_capabilities,
            desired_present_mode,
            desired_image_count,
            surface_resolution,
        })
    }
}
