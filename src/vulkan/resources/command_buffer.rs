use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::{self, vk};

use crate::error::Error;

use super::{command_pool::CommandPool, device::Device};

pub struct CommandBuffer {
    device: Rc<Device>,
    command_pool: Rc<CommandPool>,
    command_buffer: vk::CommandBuffer,
}

impl Deref for CommandBuffer {
    type Target = vk::CommandBuffer;

    fn deref(&self) -> &Self::Target {
        &self.command_buffer
    }
}

impl CommandBuffer {
    pub fn new(device: &Rc<Device>, command_pool: &Rc<CommandPool>) -> Result<Rc<Self>, Error> {
        debug!("Creating command buffer");
        let device = device.clone();
        let command_pool = command_pool.clone();

        let buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_buffer_count(1)
            .command_pool(**command_pool)
            .level(vk::CommandBufferLevel::PRIMARY);

        let command_buffers_or_err =
            unsafe { device.allocate_command_buffers(&buffer_allocate_info) };

        let command_buffer = command_buffers_or_err.map(|some| some[0])?;

        Ok(Rc::new(CommandBuffer {
            device,
            command_pool,
            command_buffer,
        }))
    }
}

impl Drop for CommandBuffer {
    fn drop(&mut self) {
        debug!("Destroying command buffer");
        unsafe {
            self.device
                .free_command_buffers(**self.command_pool, &[**self]);
        }
    }
}
