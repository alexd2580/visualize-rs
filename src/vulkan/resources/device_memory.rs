use std::{ops::Deref, rc::Rc};

use ash::{self, vk};

use crate::error::Error;

use super::{device::Device, physical_device::PhysicalDevice};

pub struct DeviceMemory {
    device: Rc<Device>,
    size: vk::DeviceSize,
    memory: vk::DeviceMemory,
}

impl Deref for DeviceMemory {
    type Target = vk::DeviceMemory;

    fn deref(&self) -> &Self::Target {
        &self.memory
    }
}

impl DeviceMemory {
    pub unsafe fn new(
        physical_device: &PhysicalDevice,
        device: &Rc<Device>,
        size: vk::DeviceSize,
    ) -> Result<Rc<Self>, Error> {
        let device = device.clone();
        let memory_type_index = physical_device.memory_type_index;
        let memory_alloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(size)
            .memory_type_index(memory_type_index);
        let memory = device.allocate_memory(&memory_alloc_info, None)?;
        Ok(Rc::new(DeviceMemory {
            device,
            size,
            memory,
        }))
    }

    pub fn size(&self) -> vk::DeviceSize {
        self.size
    }
}

impl Drop for DeviceMemory {
    fn drop(&mut self) {
        unsafe {
            self.device.free_memory(**self, None);
        }
    }
}
