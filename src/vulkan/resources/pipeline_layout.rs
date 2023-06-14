use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::{descriptor_layout::DescriptorLayout, device::Device, shader_module::ShaderModule};

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
        descriptor_layout: &DescriptorLayout,
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

        let layouts = [**descriptor_layout];
        let layout_create_info = vk::PipelineLayoutCreateInfo::builder()
            .push_constant_ranges(&push_constant_ranges)
            .set_layouts(&layouts);
        let pipeline_layout = device.create_pipeline_layout(&layout_create_info, None)?;

        Ok(Rc::new(PipelineLayout {
            device,
            pipeline_layout,
        }))
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
