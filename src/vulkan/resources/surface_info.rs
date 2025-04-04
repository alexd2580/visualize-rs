use tracing::{debug, error, warn};

use ash::{extensions::khr::Surface as SurfaceLoader, vk};

use crate::error::Error;

use super::{physical_device::PhysicalDevice, surface::Surface};

#[derive(Debug)]
pub struct SurfaceInfo {
    pub surface_format: vk::SurfaceFormatKHR,
    pub surface_capabilities: vk::SurfaceCapabilitiesKHR,
    pub desired_present_mode: vk::PresentModeKHR,
    pub desired_image_count: usize,
    pub surface_resolution: vk::Extent2D,
}

impl SurfaceInfo {
    pub unsafe fn new(
        physical_device: &PhysicalDevice,
        surface_loader: &SurfaceLoader,
        surface: &Surface,
        vsync: bool,
    ) -> Result<Self, Error> {
        debug!("Collecting surface info");

        // dbg!(&**physical_device, &**surface, physical_device.compute_queue_family_index);
        let present_support = surface_loader.get_physical_device_surface_support(
            **physical_device,
            physical_device.compute_queue_family_index,
            **surface,
        )?;
        // dbg!(&present_support);
        if !present_support {
            dbg!("RIP");
        }

        let surface_formats =
            surface_loader.get_physical_device_surface_formats(**physical_device, **surface)?;
        let surface_capabilities = surface_loader
            .get_physical_device_surface_capabilities(**physical_device, **surface)?;
        let present_modes = surface_loader
            .get_physical_device_surface_present_modes(**physical_device, **surface)?;

        let surface_format = surface_formats[0];

        // For reference see:
        // https://www.reddit.com/r/vulkan/comments/9txqqb/what_is_presentation_mode/

        let desired_present_mode = if vsync {
            vk::PresentModeKHR::FIFO
        } else {
            vk::PresentModeKHR::IMMEDIATE
        };

        let desired_present_mode = present_modes
            .into_iter()
            .find(|&mode| mode == desired_present_mode)
            .ok_or_else(|| Error::Local("There is no vsync present mode".to_owned()))?;

        // Check that the surface supports storage write/can be used in compute shaders.
        if !surface_capabilities
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED)
        {
            return Err(Error::Local(
                "Surface cannot be used for storage".to_owned(),
            ));
        }

        // Try to get triple buffering, fall back to double-buffering.
        // Assuming all modern GPUs support double buffering.
        let min_image_count = surface_capabilities.min_image_count as usize;
        let max_image_count = surface_capabilities.max_image_count as usize;
        let mut desired_image_count = min_image_count + 1;
        if max_image_count != 0 && desired_image_count > max_image_count {
            desired_image_count = max_image_count;
        }

        let mut surface_resolution = surface_capabilities.current_extent;
        if surface_resolution.width == std::u32::MAX {
            error!("Unexpected situation: {surface_capabilities:#?}");
            warn!("Setting surface resolution to HD");
            surface_resolution = vk::Extent2D {
                width: 1280,
                height: 720,
            };
        }
        debug!("Collecting surface info done");

        Ok(SurfaceInfo {
            surface_format,
            surface_capabilities,
            desired_present_mode,
            desired_image_count,
            surface_resolution,
        })
    }
}
