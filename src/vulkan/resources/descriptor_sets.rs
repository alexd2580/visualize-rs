use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::{
    descriptor_pool::DescriptorPool, descriptor_set_layout::DescriptorSetLayout, device::Device,
};

pub struct DescriptorSets {
    descriptor_sets: Vec<vk::DescriptorSet>,
}

impl Deref for DescriptorSets {
    type Target = [vk::DescriptorSet];

    fn deref(&self) -> &Self::Target {
        &self.descriptor_sets
    }
}

impl DescriptorSets {
    pub unsafe fn new(
        device: &Device,
        descriptor_set_layout: &DescriptorSetLayout,
        descriptor_pool: &DescriptorPool,
        num_sets: u32,
    ) -> Result<Rc<Self>, Error> {
        debug!("Creating descriptor sets");

        let descriptor_set_layout = **descriptor_set_layout;
        let descriptor_set_layouts = vec![descriptor_set_layout; num_sets as usize];
        let descriptor_set_allocate_info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(**descriptor_pool)
            .set_layouts(&descriptor_set_layouts);

        let descriptor_sets = device.allocate_descriptor_sets(&descriptor_set_allocate_info)?;

        Ok(Rc::new(DescriptorSets { descriptor_sets }))
    }
}
