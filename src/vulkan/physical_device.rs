use crate::error::Error;
use ash::vk;
use core::ops::Not;
use log::debug;

use super::{instance::Instance, surface::Surface};

fn choose_compute_queue_family(
    _physical_device: vk::PhysicalDevice,
    (index, queue_family_properties): (usize, &vk::QueueFamilyProperties),
) -> Option<u32> {
    let queue_flags = queue_family_properties.queue_flags;
    let supports_compute = queue_flags.contains(vk::QueueFlags::COMPUTE);
    let does_not_support_graphics = queue_flags.not().contains(vk::QueueFlags::GRAPHICS);

    if supports_compute && does_not_support_graphics {
        Some(index as u32)
    } else {
        None
    }
}

// fn choose_render_queue_family(
//     surface: &Surface,
//     physical_device: vk::PhysicalDevice,
//     (index, queue_family_properties): (usize, &vk::QueueFamilyProperties),
// ) -> Option<u32> {
//     let supports_graphics = queue_family_properties
//         .queue_flags
//         .contains(vk::QueueFlags::GRAPHICS);
//     let supports_surface = unsafe {
//         surface.surface_loader.get_physical_device_surface_support(
//             physical_device,
//             index as u32,
//             surface.surface,
//         )
//     }
//     .expect("Failed to get physical device surface support info");
//
//     if supports_graphics && supports_surface {
//         Some(index as u32)
//     } else {
//         None
//     }
// }

/// Search for a compute queue and a render queue in a physical device.
fn choose_physical_device_queue(
    instance: &Instance,
    _surface: &Surface,
    physical_device: vk::PhysicalDevice,
) -> Option<(vk::PhysicalDevice, u32)> {
    let queue_family_properties = unsafe {
        instance
            .instance
            .get_physical_device_queue_family_properties(physical_device)
    };

    let compute_queue_family_index = queue_family_properties
        .iter()
        .enumerate()
        .find_map(|queue_family| choose_compute_queue_family(physical_device, queue_family))?;

    Some((physical_device, compute_queue_family_index))
}

pub struct PhysicalDevice {
    pub physical_device: vk::PhysicalDevice,
    pub compute_queue_family_index: u32,
}

impl PhysicalDevice {
    pub fn new(instance: &Instance, surface: &Surface) -> Result<PhysicalDevice, Error> {
        debug!("Choosing physical device");
        let physical_devices = instance.enumerate_physical_devices()?;
        let (physical_device, compute_queue_family_index) = physical_devices
            .into_iter()
            .find_map(|p| choose_physical_device_queue(instance, surface, p))
            .ok_or_else(|| Error::Local("Couldn't find suitable device".to_owned()))?;

        Ok(PhysicalDevice {
            physical_device,
            compute_queue_family_index,
        })
    }
}
