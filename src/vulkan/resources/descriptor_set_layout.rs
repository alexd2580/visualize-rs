use std::{collections::HashMap, ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::{descriptor_set_layout_bindings::DescriptorSetLayoutBindings, device::Device};

pub struct DescriptorSetLayout {
    device: Rc<Device>,
    descriptor_set_layout: vk::DescriptorSetLayout,
}

impl Deref for DescriptorSetLayout {
    type Target = vk::DescriptorSetLayout;

    fn deref(&self) -> &Self::Target {
        &self.descriptor_set_layout
    }
}

impl DescriptorSetLayout {
    pub unsafe fn new(
        device: &Rc<Device>,
        descriptor_set_layout_binding_sets: &HashMap<u32, DescriptorSetLayoutBindings>,
    ) -> Result<Rc<HashMap<u32, Self>>, Error> {
        debug!("Creating descriptor set layout");
        let set_layouts = descriptor_set_layout_binding_sets
            .iter()
            .map(|(&set_index, bindings)| {
                let device = device.clone();
                let descriptor_set_layout_create_info =
                    vk::DescriptorSetLayoutCreateInfo::builder().bindings(bindings);
                let descriptor_set_layout = device
                    .create_descriptor_set_layout(&descriptor_set_layout_create_info, None)?;

                Ok((
                    set_index,
                    DescriptorSetLayout {
                        device,
                        descriptor_set_layout,
                    },
                ))
            })
            // Funny way of converting a iter<result> into result<hashmap>.
            .collect::<Result<Vec<(u32, DescriptorSetLayout)>, Error>>()?
            .into_iter()
            .collect();

        Ok(Rc::new(set_layouts))
    }
}

impl Drop for DescriptorSetLayout {
    fn drop(&mut self) {
        debug!("Dropping descriptor set layout");
        unsafe {
            self.device.destroy_descriptor_set_layout(**self, None);
        }
    }
}
