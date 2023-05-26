use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

pub struct DescriptorSetLayoutBindings {
    descriptor_set_layout_bindings: Vec<vk::DescriptorSetLayoutBinding>,
}

impl Deref for DescriptorSetLayoutBindings {
    type Target = Vec<vk::DescriptorSetLayoutBinding>;

    fn deref(&self) -> &Self::Target {
        &self.descriptor_set_layout_bindings
    }
}

impl DescriptorSetLayoutBindings {
    fn make_binding(
        binding: u32,
        descriptor_type: vk::DescriptorType,
    ) -> vk::DescriptorSetLayoutBinding {
        vk::DescriptorSetLayoutBinding {
            binding,
            descriptor_type,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::COMPUTE,
            ..Default::default()
        }
    }

    pub fn new() -> Result<Rc<Self>, Error> {
        debug!("Creating descriptor set layout bindings");

        let present_image = Self::make_binding(0, vk::DescriptorType::STORAGE_IMAGE);
        let dft = Self::make_binding(1, vk::DescriptorType::STORAGE_BUFFER);

        // TODO immutable samplers?
        let descriptor_set_layout_bindings = vec![present_image, dft];

        Ok(Rc::new(DescriptorSetLayoutBindings {
            descriptor_set_layout_bindings,
        }))
    }
}
