use std::{ops::Deref, rc::Rc};

use tracing::debug;

use ash::{self, vk};

use crate::error::Error;

use super::{device::Device, physical_device::PhysicalDevice};

pub struct CommandPool {
    device: Rc<Device>,
    command_pool: vk::CommandPool,
}

impl Deref for CommandPool {
    type Target = vk::CommandPool;

    fn deref(&self) -> &Self::Target {
        &self.command_pool
    }
}

impl CommandPool {
    pub unsafe fn new(
        physical_device: &PhysicalDevice,
        device: &Rc<Device>,
    ) -> Result<Rc<Self>, Error> {
        debug!("Creating command pool");
        let device = device.clone();

        let pool_create_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(physical_device.compute_queue_family_index);

        let command_pool = device.create_command_pool(&pool_create_info, None)?;

        Ok(Rc::new(CommandPool {
            device,
            command_pool,
        }))
    }
}

impl Drop for CommandPool {
    fn drop(&mut self) {
        debug!("Destroying command pool");
        unsafe {
            self.device.destroy_command_pool(**self, None);
        }
    }
}
