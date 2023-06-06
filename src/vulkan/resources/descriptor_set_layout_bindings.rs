use std::{collections::HashMap, ops::Deref, rc::Rc};

use log::{debug, warn};

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

fn collect_partitions<Key: std::cmp::Eq + std::hash::Hash + Clone, Value: std::fmt::Debug>(
    iter: impl Iterator<Item = (Key, Value)>,
) -> HashMap<Key, Vec<Value>> {
    let mut hash_map = HashMap::new();
    for (key, value) in iter {
        let prev = match hash_map.get_mut(&key) {
            Some(prev) => prev,
            None => {
                hash_map.insert(key.clone(), Vec::new());
                hash_map.get_mut(&key)
            }
            .unwrap(),
        };

        prev.push(value);
    }
    hash_map
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
            .map(|declaration| {
                (
                    declaration.set.unwrap_or_else(|| {
                        warn!("Assuming set=0 for block {}", declaration.name);
                        0
                    }),
                    Self::make_binding(
                        declaration.binding as u32,
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
                    declaration.set.unwrap_or_else(|| {
                        warn!("Assuming set=0 for block {}", declaration.name);
                        0
                    }),
                    // Unwrap is safe, we have filtered before.
                    Self::make_binding(declaration.binding.unwrap(), declaration.storage),
                )
            });

        let partitions = collect_partitions(variable_bindings.chain(block_bindings))
            .drain()
            .map(|(k, v)| (k, DescriptorSetLayoutBindings(v)))
            .collect();

        Ok(Rc::new(partitions))
    }
}
