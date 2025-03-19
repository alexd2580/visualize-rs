use std::{ffi::CStr, ops::Deref, rc::Rc};

use log::debug;

use ash::{self, vk};

use crate::error::Error;
use crate::window::Window;

pub struct Instance {
    pub instance: ash::Instance,
}

impl Deref for Instance {
    type Target = ash::Instance;

    fn deref(&self) -> &Self::Target {
        &self.instance
    }
}

impl Instance {
    pub unsafe fn new(window: &Rc<Window>, entry: &ash::Entry) -> Result<Rc<Self>, Error> {
        debug!("Creating instance");
        let app_info = vk::ApplicationInfo::builder().api_version(vk::make_api_version(0, 1, 3, 0));

        // Instance extensions.
        let extension_names = window.enumerate_required_extensions()?;
        for e in &extension_names {
            debug!("Enabled instance extension {:?}", CStr::from_ptr(*e));
        }

        // List available layers. TODO check that the validation layer exists.
        // let layer_properties = entry
        //     .enumerate_instance_layer_properties()?;
        let validation_layer =
            CStr::from_bytes_with_nul_unchecked(b"VK_LAYER_KHRONOS_validation\0");
        let layer_names = [validation_layer.as_ptr()];
        for l in &layer_names {
            debug!("Enabled layer {:?}", CStr::from_ptr(*l));
        }

        let create_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names)
            .enabled_layer_names(&layer_names);

        let instance = entry.create_instance(&create_info, None)?;
        Ok(Rc::new(Instance { instance }))
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        debug!("Destroying instance");
        unsafe {
            self.destroy_instance(None);
        }
    }
}
