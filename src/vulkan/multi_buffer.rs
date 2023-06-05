use std::{ffi::c_void, rc::Rc};

use ash::{self, vk};
use log::debug;

use crate::error::Error;

use super::resources::{
    buffer::Buffer, device::Device, device_memory::DeviceMemory, memory_mapping::MemoryMapping,
    physical_device::PhysicalDevice,
};

pub struct MultiBufferUnit {
    pub buffer: Rc<Buffer>,
    pub memory: Rc<DeviceMemory>,
    pub mapping: Rc<MemoryMapping>,
}

impl MultiBufferUnit {
    pub unsafe fn new(
        physical_device: &PhysicalDevice,
        device: &Rc<Device>,
        size: vk::DeviceSize,
    ) -> Result<Self, Error> {
        let buffer = Buffer::new(device, size)?;
        let memory = DeviceMemory::new(physical_device, device, buffer.get_required_memory_size())?;
        let mapping = MemoryMapping::new(device, &memory)?;

        device.bind_buffer_memory(**buffer, **memory, 0)?;

        Ok(MultiBufferUnit {
            buffer,
            memory,
            mapping,
        })
    }
}

/// A buffer is composed of multiple device buffers used for multi-buffering (i.e.
/// triple-buffering). These buffers are automatically mapped to system memory to be written to,
/// and unmapped when the object is dropped.
pub struct MultiBuffer {
    pub buffers: Vec<MultiBufferUnit>,
}

impl MultiBuffer {
    pub unsafe fn new(
        physical_device: &Rc<PhysicalDevice>,
        device: &Rc<Device>,
        size: vk::DeviceSize,
        num_buffers: usize,
    ) -> Result<Rc<Self>, Error> {
        debug!("Creating buffer of size {}", size);
        let buffers = (0..num_buffers)
            .map(|_| MultiBufferUnit::new(physical_device, device, size))
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(Rc::new(MultiBuffer { buffers }))
    }

    pub fn mapped(&self, index: usize) -> *mut c_void {
        **self.buffers[index].mapping
    }
}

impl Drop for MultiBuffer {
    fn drop(&mut self) {
        debug!("Destroying buffer");
    }
}
