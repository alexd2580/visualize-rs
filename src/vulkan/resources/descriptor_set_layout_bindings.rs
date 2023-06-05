use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::shader_module::ShaderModule;

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

    pub fn new(shader_module: &ShaderModule) -> Result<Rc<Self>, Error> {
        debug!("Creating descriptor set layout bindings");

        // Are these always storage images?
        let variable_bindings = shader_module
            .variable_declarations
            .iter()
            .map(|declaration| {
                Self::make_binding(
                    declaration.binding as u32,
                    vk::DescriptorType::STORAGE_IMAGE,
                )
            });

        let block_bindings = shader_module
            .block_declarations
            .iter()
            .filter_map(|declaration| {
                declaration
                    .binding
                    .map(|binding| Self::make_binding(binding, declaration.storage))
            });

        // TODO immutable samplers, what are immutable samplers???
        let descriptor_set_layout_bindings = variable_bindings.chain(block_bindings).collect();
        Ok(Rc::new(DescriptorSetLayoutBindings {
            descriptor_set_layout_bindings,
        }))
    }
}
