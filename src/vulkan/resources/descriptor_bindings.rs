use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::shader_module::{analysis::DescriptorInfo, ShaderModule};

/// Hold the layout bindings grouped by their set number.
pub type DescriptorSet = Vec<vk::DescriptorSetLayoutBinding>;
pub struct DescriptorBindings(pub Vec<DescriptorSet>);

impl Deref for DescriptorBindings {
    type Target = Vec<DescriptorSet>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DescriptorBindings {
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
        debug!("Creating descriptor bindings");

        // TODO immutable samplers, what are immutable samplers???
        // Are these always storage images?
        let vars = shader_module
            .variable_declarations
            .iter()
            .filter(|declaration| declaration.binding.is_some())
            .map(|declaration| {
                (
                    declaration.binding.unwrap(),
                    declaration.set_index(),
                    vk::DescriptorType::STORAGE_IMAGE,
                )
            });

        let blocks = shader_module
            .block_declarations
            .iter()
            .filter(|declaration| declaration.binding.is_some())
            .map(|declaration| {
                (
                    declaration.binding.unwrap(),
                    declaration.checked_set(),
                    declaration.storage,
                )
            });

        let mut descriptor_sets = Vec::new();
        for (binding, set_index, desc_type) in vars.chain(blocks) {
            if set_index >= descriptor_sets.len() {
                descriptor_sets.resize_with(set_index + 1, Vec::new);
            }
            let set = &mut descriptor_sets[set_index];

            if set.iter().any(|&(prev_binding, _)| binding == prev_binding) {
                let msg =
                    format!("Shader uses same set/binding ({set_index}/{binding}) multiple times.");
                return Err(Error::Local(msg));
            } else {
                set.push((binding, desc_type));
            }
        }

        Ok(Rc::new(DescriptorBindings(
            descriptor_sets
                .into_iter()
                .map(|descriptor_set| {
                    descriptor_set
                        .into_iter()
                        .map(|(binding, desc_type)| Self::make_binding(binding, desc_type))
                        .collect()
                })
                .collect(),
        )))
    }
}
