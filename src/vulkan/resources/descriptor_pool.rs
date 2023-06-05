use std::{collections::HashMap, ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::{descriptor_set_layout_bindings::DescriptorSetLayoutBindings, device::Device};

pub struct DescriptorPool {
    device: Rc<Device>,
    descriptor_pool: vk::DescriptorPool,
}

impl Deref for DescriptorPool {
    type Target = vk::DescriptorPool;

    fn deref(&self) -> &Self::Target {
        &self.descriptor_pool
    }
}

impl DescriptorPool {
    pub unsafe fn new(
        device: &Rc<Device>,
        descriptor_set_layout_bindings: &DescriptorSetLayoutBindings,
        set_count: u32,
    ) -> Result<Rc<Self>, Error> {
        debug!("Creating descriptor pool");
        let device = device.clone();

        let mut accumulated_bindings = HashMap::new();
        for binding in &**descriptor_set_layout_bindings {
            let &new_count = accumulated_bindings
                .get(&binding.descriptor_type)
                .unwrap_or(&1);
            accumulated_bindings.insert(binding.descriptor_type, new_count);
        }
        let descriptor_pool_sizes: Vec<vk::DescriptorPoolSize> = accumulated_bindings
            .into_iter()
            .map(
                |(ty, descriptor_count): (vk::DescriptorType, u32)| vk::DescriptorPoolSize {
                    ty,
                    descriptor_count,
                },
            )
            .collect();

        let pool_create_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&descriptor_pool_sizes)
            .max_sets(set_count); // TODO

        let descriptor_pool = device.create_descriptor_pool(&pool_create_info, None)?;

        Ok(Rc::new(DescriptorPool {
            device,
            descriptor_pool,
        }))
    }
}

impl Drop for DescriptorPool {
    fn drop(&mut self) {
        debug!("Destroying descriptor pool");
        unsafe {
            self.device.destroy_descriptor_pool(**self, None);
        }
    }
}
