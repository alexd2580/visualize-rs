use std::{ffi::CStr, ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::{device::Device, pipeline_layout::PipelineLayout, shader_module::ShaderModule};

unsafe fn as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts((p as *const T) as *const u8, ::std::mem::size_of::<T>())
}

pub struct Pipeline {
    device: Rc<Device>,
    pipeline: vk::Pipeline,
}

impl Deref for Pipeline {
    type Target = vk::Pipeline;

    fn deref(&self) -> &Self::Target {
        &self.pipeline
    }
}

impl Pipeline {
    pub unsafe fn new<PushConstants>(
        device: &Rc<Device>,
        shader_module: &ShaderModule,
        pipeline_layout: &PipelineLayout<PushConstants>,
    ) -> Result<Rc<Self>, Error> {
        debug!("Creating pipleine");
        let device = device.clone();

        let shader_entry_name = CStr::from_bytes_with_nul_unchecked(b"main\0");
        let shader_stage_create_info = vk::PipelineShaderStageCreateInfo {
            module: **shader_module,
            p_name: shader_entry_name.as_ptr(),
            stage: vk::ShaderStageFlags::COMPUTE,
            ..Default::default()
        };

        let compute_pipeline_create_info = vk::ComputePipelineCreateInfo::builder()
            .stage(shader_stage_create_info)
            .layout(**pipeline_layout)
            .build();
        let pipelines = unsafe {
            device.create_compute_pipelines(
                vk::PipelineCache::null(),
                &[compute_pipeline_create_info],
                None,
            )
        }
        .map_err(|(_pipeline, result)| Error::Vk(result))?;
        // TODO delete pipeline?

        let pipeline = pipelines[0];

        Ok(Rc::new(Pipeline { device, pipeline }))
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        debug!("Destroying pipeline");
        unsafe {
            self.device.destroy_pipeline(**self, None);
        }
    }
}
