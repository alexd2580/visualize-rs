use std::{ops::Deref, rc::Rc};

use tracing::debug;

use ash::{extensions::khr::Surface as SurfaceLoader, vk};

use crate::error::Error;

use super::{instance::Instance, surface::Surface};

fn supports_compute_and_surface(
    physical_device: vk::PhysicalDevice,
    surface_loader: &SurfaceLoader,
    surface: &Surface,
    index: u32,
    queue_family_properties: &vk::QueueFamilyProperties,
) -> bool {
    let queue_flags = queue_family_properties.queue_flags;
    let supports_compute = queue_flags.contains(vk::QueueFlags::COMPUTE | vk::QueueFlags::GRAPHICS);
    // let does_not_support_graphics = queue_flags.not().contains(vk::QueueFlags::GRAPHICS);

    let supports_surface = unsafe {
        surface_loader.get_physical_device_surface_support(physical_device, index, **surface)
    }
    .unwrap();
    dbg!(&supports_surface);

    supports_compute && supports_surface // && does_not_support_graphics
}

unsafe fn choose_physical_device_queue(
    instance: &Instance,
    surface_loader: &SurfaceLoader,
    surface: &Surface,
    physical_device: vk::PhysicalDevice,
) -> Option<(vk::PhysicalDevice, u32)> {
    let queue_family_properties =
        instance.get_physical_device_queue_family_properties(physical_device);

    let compute_queue_family_index =
        queue_family_properties
            .iter()
            .enumerate()
            .find_map(|(index, queue_family_props)| {
                let index = u32::try_from(index).unwrap();
                supports_compute_and_surface(
                    physical_device,
                    surface_loader,
                    surface,
                    index,
                    queue_family_props,
                )
                .then_some(index)
            })?;

    Some((physical_device, compute_queue_family_index))
}

pub struct PhysicalDevice {
    physical_device: vk::PhysicalDevice,
    pub compute_queue_family_index: u32,
    pub buffer_memory_type_index: u32,
    pub image_memory_type_index: u32,
}

impl Deref for PhysicalDevice {
    type Target = vk::PhysicalDevice;

    fn deref(&self) -> &Self::Target {
        &self.physical_device
    }
}

fn choose_buffer_memory_type(index: u32, memory_type: vk::MemoryType) -> Option<u32> {
    let desired_flags = vk::MemoryPropertyFlags::DEVICE_LOCAL
        | vk::MemoryPropertyFlags::HOST_VISIBLE
        | vk::MemoryPropertyFlags::HOST_COHERENT;

    if memory_type.property_flags.contains(desired_flags) {
        Some(index)
    } else {
        None
    }
}

fn choose_image_memory_type(index: u32, memory_type: vk::MemoryType) -> Option<u32> {
    let desired_flags = vk::MemoryPropertyFlags::DEVICE_LOCAL;

    if memory_type.property_flags.contains(desired_flags) {
        Some(index)
    } else {
        None
    }
}

impl PhysicalDevice {
    pub unsafe fn new(
        instance: &Instance,
        surface_loader: &SurfaceLoader,
        surface: &Surface,
    ) -> Result<Rc<PhysicalDevice>, Error> {
        debug!("Choosing physical device");

        let physical_devices = instance.enumerate_physical_devices()?;
        let (physical_device, compute_queue_family_index) = physical_devices
            .into_iter()
            .find_map(|p| choose_physical_device_queue(instance, surface_loader, surface, p))
            .ok_or_else(|| Error::Local("Couldn't find suitable device".to_owned()))?;

        // For reference see: https://github.com/Traverse-Research/gpu-allocator/blob/main/src/vulkan/mod.rs#L742
        let memory_props = instance.get_physical_device_memory_properties(physical_device);
        let memory_types = &memory_props.memory_types[..memory_props.memory_type_count as _];
        let buffer_memory_type_index = memory_types
            .iter()
            .enumerate()
            .find_map(|(index, memory_type)| {
                choose_buffer_memory_type(u32::try_from(index).unwrap(), *memory_type)
            })
            .ok_or_else(|| Error::Local("Couldn't find suitable memory type".to_owned()))?;

        let image_memory_type_index = memory_types
            .iter()
            .enumerate()
            .find_map(|(index, memory_type)| {
                choose_image_memory_type(u32::try_from(index).unwrap(), *memory_type)
            })
            .ok_or_else(|| Error::Local("Couldn't find suitable memory type".to_owned()))?;

        Ok(Rc::new(PhysicalDevice {
            physical_device,
            compute_queue_family_index,
            buffer_memory_type_index,
            image_memory_type_index,
        }))
    }
}

// fn choose_render_queue_family(
//     surface: &Surface,
//     physical_device: vk::PhysicalDevice,
//     (index, queue_family_properties): (u32, &vk::QueueFamilyProperties),
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
