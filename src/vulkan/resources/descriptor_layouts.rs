use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::{descriptor_bindings::DescriptorBindings, device::Device};

pub struct DescriptorLayouts {
    device: Rc<Device>,
    descriptor_layouts: Vec<vk::DescriptorSetLayout>,
}

impl Deref for DescriptorLayouts {
    type Target = [vk::DescriptorSetLayout];

    fn deref(&self) -> &Self::Target {
        &self.descriptor_layouts
    }
}

impl DescriptorLayouts {
    pub unsafe fn new(
        device: &Rc<Device>,
        descriptor_bindings: &DescriptorBindings,
    ) -> Result<Rc<Self>, Error> {
        debug!("Creating descriptor layouts");

        let device = device.clone();
        let descriptor_layouts = descriptor_bindings
            .iter()
            .map(|bindings| {
                let descriptor_layout_create_info =
                    vk::DescriptorSetLayoutCreateInfo::builder().bindings(bindings);

                let layout =
                    device.create_descriptor_set_layout(&descriptor_layout_create_info, None)?;
                Ok(layout)
            })
            .collect::<Result<_, Error>>()?;

        Ok(Rc::new(DescriptorLayouts {
            device,
            descriptor_layouts,
        }))
    }
}

impl Drop for DescriptorLayouts {
    fn drop(&mut self) {
        debug!("Dropping descriptor set layout");
        unsafe {
            for layout in self.iter() {
                self.device.destroy_descriptor_set_layout(*layout, None);
            }
        }
    }
}
