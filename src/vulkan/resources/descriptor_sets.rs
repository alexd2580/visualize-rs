use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::{
    descriptor_layouts::DescriptorLayouts, descriptor_pool::DescriptorPool, device::Device,
};

type DescriptorSetInstances = Vec<vk::DescriptorSet>;
pub struct DescriptorSets(Vec<DescriptorSetInstances>);

impl Deref for DescriptorSets {
    type Target = [DescriptorSetInstances];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DescriptorSets {
    pub unsafe fn new(
        device: &Device,
        descriptor_layouts: &DescriptorLayouts,
        descriptor_pool: &DescriptorPool,
        num_sets: u32,
    ) -> Result<Rc<Self>, Error> {
        debug!("Creating descriptor sets");

        let create_descriptor_set = |descriptor_layout: &vk::DescriptorSetLayout| {
            let descriptor_layout = *descriptor_layout;
            let descriptor_layouts = vec![descriptor_layout; num_sets as usize];
            let descriptor_set_allocate_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(**descriptor_pool)
                .set_layouts(&descriptor_layouts);

            Ok(device.allocate_descriptor_sets(&descriptor_set_allocate_info)?)
        };

        let descriptor_sets = descriptor_layouts
            .iter()
            .map(create_descriptor_set)
            .collect::<Result<_, Error>>()?;

        Ok(Rc::new(DescriptorSets(descriptor_sets)))
    }
}
