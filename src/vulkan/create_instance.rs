use ash::{self, extensions, vk};

use std::{ffi::CStr, ops::Deref, rc::Rc};

use crate::window::Window;
use log::debug;

use crate::error::Error;

use super::Vulkan;

// use super::physical_device::PhysicalDevice;

impl Vulkan {
    pub unsafe fn create_instance(&mut self, window: &Window) -> Result<(), Error> {
        debug!("Creating instance");

        let entry = ash::Entry::linked();

        let app_info = vk::ApplicationInfo::builder().api_version(vk::make_api_version(0, 1, 3, 0));
        let extension_names = window.enumerate_required_extensions()?;

        // List available layers. TODO check that the validation layer exists.
        // let layer_properties = entry
        //     .enumerate_instance_layer_properties()?;
        let validation_layer =
            CStr::from_bytes_with_nul_unchecked(b"VK_LAYER_KHRONOS_validation\0");
        let layer_names = [validation_layer.as_ptr()];

        let create_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names)
            .enabled_layer_names(&layer_names);

        let instance = entry.create_instance(&create_info, None)?;

        self.instance = Wrapper::from(Rc::new(Instance { entry, instance }));
        Ok(())
    }
}
