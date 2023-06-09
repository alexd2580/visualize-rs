use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::shader_module::ShaderModule;

/// Hold the layout bindings grouped by their set number.
pub struct DescriptorSetLayoutBindings(pub Vec<vk::DescriptorSetLayoutBinding>);

impl Deref for DescriptorSetLayoutBindings {
    type Target = Vec<vk::DescriptorSetLayoutBinding>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn collect_partitions<Value: std::fmt::Debug>(
    iter: impl Iterator<Item = (usize, Value)>,
) -> Vec<Vec<Value>> {
    let mut result = Vec::new();
    for (index, value) in iter {
        if index >= result.len() {
            result.resize_with(index + 1, Vec::new);
        }

        result[index].push(value);
    }
    result
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

    pub fn new(shader_module: &ShaderModule) -> Result<Rc<Vec<Self>>, Error> {
        debug!("Creating descriptor set layout bindings");

        // TODO immutable samplers, what are immutable samplers???
        // Are these always storage images?
        let variable_bindings = shader_module
            .variable_declarations
            .iter()
            .filter(|declaration| declaration.binding.is_some())
            .map(|declaration| {
                (
                    declaration.checked_set(),
                    // Unwrap is safe, we have filtered before.
                    Self::make_binding(
                        declaration.binding.unwrap(),
                        vk::DescriptorType::STORAGE_IMAGE,
                    ),
                )
            });

        // Uniform and storage buffers.
        // TODO Warn about buffers without explicit binding.
        let block_bindings = shader_module
            .block_declarations
            .iter()
            .filter(|declaration| declaration.binding.is_some())
            .map(|declaration| {
                (
                    declaration.checked_set(),
                    // Unwrap is safe, we have filtered before.
                    Self::make_binding(declaration.binding.unwrap(), declaration.storage),
                )
            });

        let partitions = collect_partitions(variable_bindings.chain(block_bindings))
            .into_iter()
            .map(DescriptorSetLayoutBindings)
            .collect();

        Ok(Rc::new(partitions))
    }
}
