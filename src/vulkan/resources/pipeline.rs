use std::{ffi::CString, ops::Deref, rc::Rc, slice::Iter};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::{device::Device, pipeline_layout::PipelineLayout, shader_module::ShaderModule};

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
    pub unsafe fn new(
        device: &Rc<Device>,
        shader_module: &ShaderModule,
        pipeline_layout: &PipelineLayout,
    ) -> Result<Rc<Self>, Error> {
        debug!("Creating pipleine");
        let device = device.clone();

        let shader_entry_name = CString::new(shader_module.main_name.as_str())
            .expect("Did not expect string conversion to fail");
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

    pub unsafe fn many(
        device: &Rc<Device>,
        shader_modules: Iter<impl Deref<Target = ShaderModule>>,
        pipeline_layouts: Iter<impl Deref<Target = PipelineLayout>>,
    ) -> Result<Vec<Rc<Self>>, Error> {
        shader_modules
            .zip(pipeline_layouts)
            .map(|(shader_module, pipeline_layout)| {
                Pipeline::new(device, shader_module, pipeline_layout)
            })
            .collect()
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
