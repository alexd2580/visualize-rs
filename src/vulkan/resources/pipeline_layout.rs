use std::{ops::Deref, rc::Rc, slice::Iter};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::{
    descriptor_set_layout::DescriptorSetLayout, device::Device, shader_module::ShaderModule,
};

pub struct PipelineLayout {
    device: Rc<Device>,
    pipeline_layout: vk::PipelineLayout,
}

impl Deref for PipelineLayout {
    type Target = vk::PipelineLayout;

    fn deref(&self) -> &Self::Target {
        &self.pipeline_layout
    }
}

impl PipelineLayout {
    pub unsafe fn new(
        device: &Rc<Device>,
        shader_module: &ShaderModule,
        descriptor_set_layouts: &[DescriptorSetLayout],
    ) -> Result<Rc<Self>, Error> {
        debug!("Creating pipeline layout");
        let device = device.clone();

        let push_constants_size = shader_module
            .push_constants_declaration()
            .map(|declaration| {
                declaration
                    .byte_size()
                    .ok_or_else(|| Error::Local("Push constant block is unsized".to_owned()))
            })
            .transpose()?;

        let mut push_constant_ranges = Vec::new();

        if let Some(size) = push_constants_size {
            push_constant_ranges.push(
                vk::PushConstantRange::builder()
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .size(size)
                    .offset(0)
                    .build(),
            );
        }

        let descriptor_set_layouts: Vec<_> = descriptor_set_layouts.iter().map(|x| **x).collect();
        let layout_create_info = vk::PipelineLayoutCreateInfo::builder()
            .push_constant_ranges(&push_constant_ranges)
            .set_layouts(&descriptor_set_layouts);
        let pipeline_layout = device.create_pipeline_layout(&layout_create_info, None)?;

        Ok(Rc::new(PipelineLayout {
            device,
            pipeline_layout,
        }))
    }

    pub unsafe fn many(
        device: &Rc<Device>,
        shader_modules: Iter<impl Deref<Target = ShaderModule>>,
        descriptor_set_layouts: &[DescriptorSetLayout],
    ) -> Result<Vec<Rc<Self>>, Error> {
        shader_modules
            .map(|module| PipelineLayout::new(device, module, descriptor_set_layouts))
            .collect()
    }
}

impl Drop for PipelineLayout {
    fn drop(&mut self) {
        debug!("Destroying pipeline layout");
        unsafe {
            self.device.destroy_pipeline_layout(**self, None);
        }
    }
}
