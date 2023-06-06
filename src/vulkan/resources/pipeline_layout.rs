use std::{marker::PhantomData, mem, ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::{descriptor_set_layout::DescriptorSetLayout, device::Device};

pub struct PipelineLayout<PushConstants> {
    _push_constants: PhantomData<PushConstants>,

    device: Rc<Device>,
    pipeline_layout: vk::PipelineLayout,
}

impl<PushConstants> Deref for PipelineLayout<PushConstants> {
    type Target = vk::PipelineLayout;

    fn deref(&self) -> &Self::Target {
        &self.pipeline_layout
    }
}

impl<PushConstants> PipelineLayout<PushConstants> {
    pub unsafe fn new(
        device: &Rc<Device>,
        descriptor_set_layout: &HashMap<u32, DescriptorSetLayout>,
    ) -> Result<Rc<Self>, Error> {
        debug!("Creating pipeline layout");
        let device = device.clone();

        let push_constants_size = mem::size_of::<PushConstants>() as u32;
        let push_constant_range = vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .size(push_constants_size)
            .offset(0)
            .build();
        let push_constant_ranges = [push_constant_range];
        let descriptor_set_layouts = [**descriptor_set_layout];
        let layout_create_info = vk::PipelineLayoutCreateInfo::builder()
            .push_constant_ranges(&push_constant_ranges)
            .set_layouts(&descriptor_set_layouts);
        let pipeline_layout = device.create_pipeline_layout(&layout_create_info, None)?;

        Ok(Rc::new(PipelineLayout {
            _push_constants: PhantomData,
            device,
            pipeline_layout,
        }))
    }
}

impl<PushConstants> Drop for PipelineLayout<PushConstants> {
    fn drop(&mut self) {
        debug!("Destroying pipeline layout");
        unsafe {
            self.device.destroy_pipeline_layout(**self, None);
        }
    }
}
