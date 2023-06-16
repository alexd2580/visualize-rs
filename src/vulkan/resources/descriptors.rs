use std::ops::{Deref, DerefMut};

use log::debug;

use ash::vk;

use crate::{
    error::Error,
    vulkan::{
        resources::shader_module::analysis::DescriptorInfo, AvailableBuffers, AvailableImages,
    },
};

use super::shader_module::ShaderModule;

fn write_descriptor_set_builder_stub(
    descriptor_binding: u32,
    storage_type: vk::DescriptorType,
) -> vk::WriteDescriptorSetBuilder<'static> {
    vk::WriteDescriptorSet::builder()
        .descriptor_type(storage_type)
        .dst_binding(descriptor_binding)
        .dst_array_element(0)
}

#[derive(Debug)]
pub struct DescriptorBinding {
    /// Name of the object.
    pub name: String,

    /// Binding index of the object (specified in the shader).
    binding: u32,

    /// The type of the underlying buffer/image.
    storage_type: vk::DescriptorType,

    /// Instances, actual data, to be bound. Created and linked in application code.
    pub instances: Vec<vk::WriteDescriptorSet>,
}

impl DescriptorBinding {
    pub fn as_descriptor_set_layout_binding(&self) -> vk::DescriptorSetLayoutBinding {
        vk::DescriptorSetLayoutBinding {
            binding: self.binding,
            descriptor_type: self.storage_type,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::COMPUTE,
            ..Default::default()
        }
    }

    fn get_write_descriptor_set_entry(
        &mut self,
        available_images: &AvailableImages,
        available_buffers: &AvailableBuffers,
        present_name: &str,
        present_index: usize,
        frame_index: usize,
    ) -> Result<vk::WriteDescriptorSet, Error> {
        if self.instances.is_empty() {
            debug!(
                "Associating buffers for binding {}: {}",
                self.binding, self.name
            );

            // TODO check storage type.

            self.instances = available_images
                .get(&self.name)
                .map(|images| {
                    images
                        .iter()
                        .map(|(_, _, image_info)| {
                            write_descriptor_set_builder_stub(self.binding, self.storage_type)
                                .image_info(image_info.as_ref())
                                .build()
                        })
                        .collect()
                })
                .or_else(|| {
                    available_buffers.get(&self.name).map(|buffers| {
                        buffers
                            .iter()
                            .map(|(_, buffer_info)| {
                                write_descriptor_set_builder_stub(self.binding, self.storage_type)
                                    .buffer_info(buffer_info.as_ref())
                                    .build()
                            })
                            .collect()
                    })
                })
                .map(Ok)
                .unwrap_or_else(|| {
                    let msg = format!("No buffer for binding {}: {}", self.binding, self.name);
                    Err(Error::Local(msg))
                })?;
        }

        let instance_index = if self.name == present_name {
            present_index
        } else {
            frame_index % self.instances.len()
        };
        Ok(self.instances[instance_index])
    }
}

/// Descriptor sets have multiple instances which can be bound. This is per-shader data, binding
/// indices do not need to be consistent across shaders. Currently the final mapping is done via
/// buffer/image name.
pub struct Descriptors(Vec<DescriptorBinding>);

impl Deref for Descriptors {
    type Target = [DescriptorBinding];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Descriptors {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Descriptors {
    pub fn new(shader_module: &ShaderModule) -> Result<Self, Error> {
        debug!("Creating descriptor bindings");

        // TODO immutable samplers, what are immutable samplers???
        // Are these always storage images?
        let vars = shader_module
            .variable_declarations
            .iter()
            .filter(|declaration| declaration.binding.is_some())
            .map(|declaration| DescriptorBinding {
                name: declaration.name.to_owned(),
                binding: declaration.binding.unwrap(),
                storage_type: declaration.storage(),
                instances: Vec::new(),
            });

        let blocks = shader_module
            .block_declarations
            .iter()
            .filter(|declaration| declaration.binding.is_some())
            .map(|declaration| DescriptorBinding {
                name: declaration.identifier.as_ref().unwrap().to_owned(),
                binding: declaration.binding.unwrap(),
                storage_type: declaration.storage,
                instances: Vec::new(),
            });

        Ok(Descriptors(vars.chain(blocks).collect()))
    }

    pub fn get_write_descriptor_set(
        &mut self,
        available_images: &AvailableImages,
        available_buffers: &AvailableBuffers,
        present_name: &str,
        present_index: usize,
        frame_index: usize,
    ) -> Result<Vec<vk::WriteDescriptorSet>, Error> {
        self.iter_mut()
            .map(|descriptor| {
                descriptor.get_write_descriptor_set_entry(
                    available_images,
                    available_buffers,
                    present_name,
                    present_index,
                    frame_index,
                )
            })
            .collect()
    }
}
