use std::rc::Rc;

use ash::vk;

use crate::{
    error::Error,
    utils::map_snd,
    vulkan::{
        resources::{
            buffer::Buffer,
            image_view::ImageView,
            sampler::Sampler,
            shader_module::analysis::{self, DescriptorInfo},
        },
        Vulkan,
    },
};

fn build_image_info(
    views_and_samplers: Vec<(Rc<ImageView>, Rc<Sampler>)>,
) -> Vec<[vk::DescriptorImageInfo; 1]> {
    views_and_samplers
        .into_iter()
        .map(|(view, sampler)| {
            [vk::DescriptorImageInfo::builder()
                .image_view(**view)
                .sampler(**sampler)
                .image_layout(vk::ImageLayout::GENERAL)
                .build()]
        })
        .collect()
}

fn build_buffer_info(buffers: Vec<Rc<Buffer>>) -> Vec<[vk::DescriptorBufferInfo; 1]> {
    buffers
        .into_iter()
        .map(|buffer| {
            [vk::DescriptorBufferInfo::builder()
                .buffer(**buffer)
                .offset(0)
                .range(buffer.size)
                .build()]
        })
        .collect()
}

impl Vulkan {
    fn variable_declaration(&self, name: &str) -> Result<&analysis::VariableDeclaration, Error> {
        self.compute_shader_modules
            .iter()
            .find_map(|module| module.variable_declaration(name))
            .ok_or_else(|| {
                let msg = format!("Declaration for {name} not found in any shader module.");
                Error::Local(msg)
            })
    }

    fn block_declaration(&self, name: &str) -> Result<&analysis::BlockDeclaration, Error> {
        self.compute_shader_modules
            .iter()
            .find_map(|module| module.block_declaration(name))
            .ok_or_else(|| {
                let msg = format!("Declaration for {name} not found in any shader module.");
                Error::Local(msg)
            })
    }

    pub unsafe fn flush_binding_updates(&mut self) -> Result<(), Error> {
        if self.image_binding_updates.is_empty() && self.buffer_binding_updates.is_empty() {
            return Ok(());
        }

        let image_descriptor_data = self
            .image_binding_updates
            .drain(0..)
            .map(map_snd(&build_image_info))
            .collect::<Vec<_>>();

        let buffer_descriptor_data = self
            .buffer_binding_updates
            .drain(0..)
            .map(map_snd(&build_buffer_info))
            .collect::<Vec<_>>();

        let mut descriptor_writes = Vec::new();

        for (name, image_infos) in &image_descriptor_data {
            let declaration = self.variable_declaration(name)?;
            let storage = declaration.storage();
            let set_index = declaration.set_index();
            let sets = &self.descriptor_sets_sets[set_index];
            let binding = declaration.binding()?;

            if sets.len() != image_infos.len() {
                let msg = format!(
                    "Specified {} values for variable {} in set {} with {} instances",
                    image_infos.len(),
                    name,
                    set_index,
                    sets.len()
                );
                return Err(Error::Local(msg));
            }

            descriptor_writes.extend(sets.iter().zip(image_infos.iter()).map(
                |(descriptor_set, image_info)| {
                    vk::WriteDescriptorSet::builder()
                        .image_info(image_info)
                        .descriptor_type(storage)
                        .dst_set(*descriptor_set)
                        .dst_binding(binding)
                        .dst_array_element(0)
                        .build()
                },
            ));
        }

        for (name, buffer_infos) in &buffer_descriptor_data {
            let declaration = self.block_declaration(name)?;
            let storage = declaration.storage();
            let set_index = declaration.set_index();
            let sets = &self.descriptor_sets_sets[set_index];
            let binding = declaration.binding()?;

            if sets.len() != buffer_infos.len() {
                let msg = format!(
                    "Specified {} values for block {} in set {} with {} instances",
                    buffer_infos.len(),
                    name,
                    set_index,
                    sets.len()
                );
                return Err(Error::Local(msg));
            }

            descriptor_writes.extend(sets.iter().zip(buffer_infos.iter()).map(
                |(descriptor_set, buffer_info)| {
                    vk::WriteDescriptorSet::builder()
                        .buffer_info(buffer_info)
                        .descriptor_type(storage)
                        .dst_set(*descriptor_set)
                        .dst_binding(binding)
                        .dst_array_element(0)
                        .build()
                },
            ))
        }

        self.device.update_descriptor_sets(&descriptor_writes, &[]);
        Ok(())
    }
}
