use ash::{self, extensions, vk};

use std::ffi::CStr;

use crate::window::Window;
use log::debug;

use crate::error::Error;

use super::physical_device::PhysicalDevice;

pub struct Instance {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
}

impl Instance {
    pub fn new(window: &Window) -> Result<Self, Error> {
        debug!("Initializing instance");
        let entry = ash::Entry::linked();

        let app_info = vk::ApplicationInfo::builder().api_version(vk::make_api_version(0, 1, 3, 0));
        let extension_names = window.enumerate_required_extensions()?;

        // List available layers. TODO check that the validation layer exists.
        let layer_properties = entry
            .enumerate_instance_layer_properties()
            .map_err(Error::VkError)?;
        let validation_layer =
            unsafe { CStr::from_bytes_with_nul_unchecked(b"VK_LAYER_KHRONOS_validation\0") };
        let layer_names = [validation_layer.as_ptr()];

        let create_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names)
            .enabled_layer_names(&layer_names);

        let instance =
            unsafe { entry.create_instance(&create_info, None) }.map_err(Error::VkError)?;

        Ok(Instance { entry, instance })
    }

    pub fn enumerate_physical_devices(&self) -> Result<Vec<vk::PhysicalDevice>, Error> {
        unsafe { self.instance.enumerate_physical_devices() }.map_err(Error::VkError)
    }

    pub fn create_device(&self, physical_device: &PhysicalDevice) -> Result<ash::Device, Error> {
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

        unsafe {
            self.instance
                .create_device(physical_device.physical_device, &device_create_info, None)
        }
        .map_err(Error::VkError)
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        debug!("Destroying instance");
        unsafe { self.instance.destroy_instance(None) };
    }
}
