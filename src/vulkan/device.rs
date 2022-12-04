use std::ops::Deref;

use ash::{self, vk};
use log::debug;

use crate::error::Error;

use super::{
    fence::Fence, instance::Instance, physical_device::PhysicalDevice, pipeline::Pipeline,
    semaphore::Semaphore,
};

pub struct Device {
    pub device: ash::Device,
    pub compute_queue: vk::Queue,
    command_pool: vk::CommandPool,
    pub command_buffer: vk::CommandBuffer,
}

impl Device {
    fn create_command_pool(
        device: &ash::Device,
        queue_family_index: u32,
    ) -> Result<vk::CommandPool, Error> {
        let pool_create_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family_index);

        unsafe { device.create_command_pool(&pool_create_info, None) }.map_err(Error::VkError)
    }

    fn create_command_buffer(
        device: &ash::Device,
        pool: vk::CommandPool,
    ) -> Result<vk::CommandBuffer, Error> {
        let buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_buffer_count(1)
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY);

        let command_buffers_or_err =
            unsafe { device.allocate_command_buffers(&buffer_allocate_info) };

        command_buffers_or_err
            .map(|some| some[0])
            .map_err(Error::VkError)
    }

    pub fn new(instance: &Instance, physical_device: &PhysicalDevice) -> Result<Self, Error> {
        let device = instance.create_device(physical_device)?;
        let compute_queue =
            unsafe { device.get_device_queue(physical_device.compute_queue_family_index, 0) };
        let command_pool =
            Self::create_command_pool(&device, physical_device.compute_queue_family_index)?;
        let command_buffer = Self::create_command_buffer(&device, command_pool)?;

        Ok(Device {
            device,
            compute_queue,
            command_pool,
            command_buffer,
        })
    }

    pub fn begin_command_buffer(&self) {
        let command_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.device
                .begin_command_buffer(self.command_buffer, &command_buffer_begin_info)
        }
        .expect("Failed to begin command buffer.");
    }

    pub fn bind_pipeline<PC>(&self, pipeline: &Pipeline<PC>) {
        unsafe {
            self.device.cmd_bind_pipeline(
                self.command_buffer,
                vk::PipelineBindPoint::COMPUTE,
                **pipeline,
            )
        };
    }

    pub fn image_memory_barrier_layout_transition(
        &self,
        image: vk::Image,
        subresource_range: vk::ImageSubresourceRange,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) {
        let memory_barrier = vk::ImageMemoryBarrier::builder()
            .image(image)
            .subresource_range(subresource_range)
            .old_layout(old_layout)
            .new_layout(new_layout)
            .build();
        let memory_barriers = [memory_barrier];

        unsafe {
            self.device.cmd_pipeline_barrier(
                self.command_buffer,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::BY_REGION,
                &[],
                &[],
                &memory_barriers,
            )
        };
    }

    pub fn dispatch(&self, x: u32, y: u32, z: u32) {
        unsafe { self.device.cmd_dispatch(self.command_buffer, x, y, z) };
    }

    pub fn end_command_buffer(&self) {
        unsafe { self.device.end_command_buffer(self.command_buffer) }
            .expect("Failed to end command buffer.");
    }

    pub fn queue_submit(
        &self,
        wait_semaphore: Option<&Semaphore>,
        signal_semaphore: Option<&Semaphore>,
        reuse_command_buffer_fence: &Fence,
    ) {
        let command_buffers = [self.command_buffer];
        let wait_semaphores = Vec::from_iter(wait_semaphore.into_iter().map(|sem| **sem));
        let signal_semaphores = Vec::from_iter(signal_semaphore.into_iter().map(|sem| **sem));

        let submit_info = vk::SubmitInfo::builder()
            .command_buffers(&command_buffers)
            .wait_semaphores(&wait_semaphores)
            .signal_semaphores(&signal_semaphores)
            .build();

        unsafe {
            self.device.queue_submit(
                self.compute_queue,
                &[submit_info],
                **reuse_command_buffer_fence,
            )
        }
        .expect("Failed to submit command buffer to queue.");
    }
}

impl Deref for Device {
    type Target = ash::Device;

    fn deref(&self) -> &Self::Target {
        &self.device
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        debug!("Destroying device");
        unsafe {
            self.free_command_buffers(self.command_pool, &[self.command_buffer]);
            self.destroy_command_pool(self.command_pool, None);
            self.destroy_device(None);
        }
    }
}
