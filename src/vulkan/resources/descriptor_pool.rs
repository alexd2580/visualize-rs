use std::{collections::HashMap, ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::{descriptor_bindings::DescriptorBindings, device::Device};

/// Pool holding memory for allocation of descriptors. Does not allocate the memory that the
/// descriptors are backed with, only the descriptors themselves.
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
        descriptor_bindings: &DescriptorBindings,
        set_count: u32,
    ) -> Result<Rc<Self>, Error> {
        // TODO Check the way descriptors are allocated (set count, descriptor count etc.).
        debug!("Creating descriptor pool");
        let device = device.clone();

        let mut accumulated_bindings = HashMap::new();
        for binding_set in descriptor_bindings.iter() {
            for binding in binding_set {
                let &old_count = accumulated_bindings
                    .get(&binding.descriptor_type)
                    .unwrap_or(&0);
                accumulated_bindings.insert(binding.descriptor_type, old_count + 1);
            }
        }

        let descriptor_pool_sizes: Vec<vk::DescriptorPoolSize> = accumulated_bindings
            .into_iter()
            .map(|(type_, count)| vk::DescriptorPoolSize {
                ty: type_,
                descriptor_count: count * set_count,
            })
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
