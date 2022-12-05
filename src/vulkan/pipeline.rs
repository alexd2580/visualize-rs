use crate::error::Error;
use ash::vk::{self, ShaderStageFlags};

use log::debug;

use std::{marker::PhantomData, mem, ops::Deref, rc::Rc};

use super::{device::Device, shader_module::ShaderModule};

unsafe fn as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts((p as *const T) as *const u8, ::std::mem::size_of::<T>())
}

pub struct Pipeline<PushConstants> {
    _push_constants: PhantomData<PushConstants>,

    device: Rc<Device>,
    pipeline_layout: vk::PipelineLayout,
    compute_pipeline: vk::Pipeline,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    pub descriptor_sets: Vec<vk::DescriptorSet>,
}

impl<PushConstants> Pipeline<PushConstants> {
    fn create_descriptor_set_layout(
        device: &ash::Device,
    ) -> Result<vk::DescriptorSetLayout, Error> {
        let descriptor_set_layout_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE);
        // TODO immutable samplers?
        let descriptor_set_layout_bindings = [*descriptor_set_layout_binding];
        let descriptor_set_layout_create_info =
            vk::DescriptorSetLayoutCreateInfo::builder().bindings(&descriptor_set_layout_bindings);
        unsafe { device.create_descriptor_set_layout(&descriptor_set_layout_create_info, None) }
            .map_err(Error::Vk)
    }

    fn create_compute_pipeline_layout(
        device: &ash::Device,
        descriptor_set_layout: &vk::DescriptorSetLayout,
    ) -> Result<vk::PipelineLayout, Error> {
        let push_constants_size = mem::size_of::<PushConstants>() as u32;
        let push_constant_range = vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .size(push_constants_size)
            .offset(0)
            .build();
        let push_constant_ranges = [push_constant_range];
        let descriptor_set_layouts = [*descriptor_set_layout];
        let layout_create_info = vk::PipelineLayoutCreateInfo::builder()
            .push_constant_ranges(&push_constant_ranges)
            .set_layouts(&descriptor_set_layouts);
        unsafe { device.create_pipeline_layout(&layout_create_info, None) }.map_err(Error::Vk)
    }

    fn create_compute_pipeline(
        device: &Device,
        pipeline_layout: &vk::PipelineLayout,
        shader_module: &ShaderModule,
    ) -> Result<vk::Pipeline, Error> {
        let compute_pipeline_create_info = vk::ComputePipelineCreateInfo::builder()
            .stage(shader_module.shader_stage_create_info)
            .layout(*pipeline_layout)
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

        Ok(pipelines[0])
    }

    pub fn create_descriptor_pool(
        device: &Device,
        descriptor_count: u32,
        set_count: u32,
    ) -> Result<vk::DescriptorPool, Error> {
        let descriptor_pool_size = vk::DescriptorPoolSize::builder()
            .ty(vk::DescriptorType::STORAGE_IMAGE)
            .descriptor_count(descriptor_count); // TODO

        let descriptor_pool_sizes = [*descriptor_pool_size];
        let pool_create_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&descriptor_pool_sizes)
            .max_sets(set_count); // TODO

        unsafe { device.create_descriptor_pool(&pool_create_info, None) }.map_err(Error::Vk)
    }

    fn create_descriptor_sets(
        device: &ash::Device,
        descriptor_pool: vk::DescriptorPool,
        descriptor_set_layout: vk::DescriptorSetLayout,
    ) -> Result<Vec<vk::DescriptorSet>, Error> {
        let descriptor_set_layouts = vec![descriptor_set_layout; 3];
        let descriptor_set_allocate_info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(descriptor_pool)
            .set_layouts(descriptor_set_layouts.as_slice());

        unsafe { device.allocate_descriptor_sets(&descriptor_set_allocate_info) }.map_err(Error::Vk)
    }

    pub fn new(device: Rc<Device>, compute_shader: &ShaderModule) -> Result<Self, Error> {
        debug!("Creating Pipleine");

        let descriptor_set_layout = Self::create_descriptor_set_layout(&device)?;

        let pipeline_layout =
            Self::create_compute_pipeline_layout(&device, &descriptor_set_layout)?;
        let compute_pipeline =
            Self::create_compute_pipeline(&device, &pipeline_layout, compute_shader)?;

        let descriptor_pool = Self::create_descriptor_pool(&device, 3, 3)?;
        let descriptor_sets =
            Self::create_descriptor_sets(&device, descriptor_pool, descriptor_set_layout)?;

        Ok(Pipeline {
            _push_constants: PhantomData,
            device,
            pipeline_layout,
            compute_pipeline,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_sets,
        })
    }

    pub fn push_constants(&self, push_constants: &PushConstants) {
        let constants = unsafe { as_u8_slice(push_constants) };
        unsafe {
            self.device.cmd_push_constants(
                self.device.command_buffer,
                self.pipeline_layout,
                ShaderStageFlags::COMPUTE,
                0,
                constants,
            )
        };
    }

    pub fn bind_descriptor_set(&self, descriptor_set_index: usize) {
        let bind_descriptor_sets = [self.descriptor_sets[descriptor_set_index]];
        unsafe {
            self.device.cmd_bind_descriptor_sets(
                self.device.command_buffer,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout,
                0,
                &bind_descriptor_sets,
                &[],
            )
        };
    }
}

//
//     let write_descriptor_sets: Vec<WriteDescriptorSet> = image_infos_vec
//         .iter()
//         .zip(descriptor_sets.iter())
//         .map(|(image_infos, &descriptor_set)| {
//             let a = WriteDescriptorSet::builder()
//                 .descriptor_type(DescriptorType::STORAGE_IMAGE)
//                 .image_info(image_infos)
//                 .dst_set(descriptor_set)
//                 .dst_binding(0)
//                 .dst_array_element(0)
//                 .build();
//
//             return a;
//         })
//         .collect();
//
//     unsafe { device.update_descriptor_sets(&write_descriptor_sets, &[]) };
//

impl<PC> Deref for Pipeline<PC> {
    type Target = vk::Pipeline;

    fn deref(&self) -> &Self::Target {
        &self.compute_pipeline
    }
}

impl<PC> Drop for Pipeline<PC> {
    fn drop(&mut self) {
        debug!("Dropping Pipeline");

        unsafe {
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            self.device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device.destroy_pipeline(self.compute_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
        }

        debug!("Pipeline dropped");
    }
}
