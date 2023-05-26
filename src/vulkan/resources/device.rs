use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::{self, extensions, vk};

use crate::error::Error;

use super::{instance::Instance, physical_device::PhysicalDevice};

pub struct Device {
    device: ash::Device,
}

impl Deref for Device {
    type Target = ash::Device;

    fn deref(&self) -> &Self::Target {
        &self.device
    }
}

impl Device {
    pub unsafe fn new(
        instance: &Instance,
        physical_device: &PhysicalDevice,
    ) -> Result<Rc<Self>, Error> {
        debug!("Creating device");

        let compute_queue_create_info = vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(physical_device.compute_queue_family_index)
            .queue_priorities(&[1.0])
            .build();

        let create_infos = &[compute_queue_create_info];
        let device_extension_names_raw = [extensions::khr::Swapchain::name().as_ptr()];
        let features = vk::PhysicalDeviceFeatures::default();

        let device_create_info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(create_infos)
            .enabled_extension_names(&device_extension_names_raw)
            .enabled_features(&features);

        let device =
            instance.create_device(physical_device.physical_device, &device_create_info, None)?;

        Ok(Rc::new(Device { device }))
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        debug!("Destroying device");
        unsafe {
            self.destroy_device(None);
        }
    }
}
