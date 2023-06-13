use std::{collections::HashMap, ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::shader_module::{analysis::DescriptorInfo, ShaderModule};

/// Hold the layout bindings grouped by their set number.
pub struct DescriptorSetLayoutBindings(pub Vec<vk::DescriptorSetLayoutBinding>);

impl Deref for DescriptorSetLayoutBindings {
    type Target = Vec<vk::DescriptorSetLayoutBinding>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn collect_set_bindings(
    iter: impl Iterator<Item = (u32, usize, vk::DescriptorType)>,
) -> Result<Vec<HashMap<u32, vk::DescriptorType>>, Error> {
    let mut sets = Vec::new();
    for (binding, set, desc_type) in iter {
        if set >= sets.len() {
            sets.resize_with(set + 1, HashMap::new);
        }

        let set = &mut sets[set];
        match set.get(&binding) {
            None => {
                set.insert(binding, desc_type);
            }
            Some(prev_desc_type) => {
                if desc_type != *prev_desc_type {
                    let msg = format!("Shaders use same set/binding with different descriptor types: {desc_type:?} and {:?}", *prev_desc_type);
                    return Err(Error::Local(msg));
                }
            }
        };
    }
    Ok(sets)
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

    pub fn new(
        shader_modules: &[impl Deref<Target = ShaderModule>],
    ) -> Result<Rc<Vec<Self>>, Error> {
        debug!("Creating descriptor set layout bindings");

        // TODO immutable samplers, what are immutable samplers???
        // Are these always storage images?
        let bindings = shader_modules.iter().flat_map(|shader_module| {
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

            vars.chain(blocks)
        });

        let partitions = collect_set_bindings(bindings)?
            .into_iter()
            .map(|set| {
                set.into_iter()
                    .map(|(binding, desc_type)| Self::make_binding(binding, desc_type))
                    .collect()
            })
            .map(DescriptorSetLayoutBindings)
            .collect();

        Ok(Rc::new(partitions))
    }
}
