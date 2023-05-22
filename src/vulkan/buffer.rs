use std::{ffi::c_void, ops::Deref, rc::Rc};

use ash::{self, vk};
use log::debug;

use crate::error::Error;

use super::device::Device;

/// Wrap a vk::Buffer so that it auto-deletes when exiting scope.
struct WrappedVkBuffer {
    device: Rc<Device>,
    buffer: vk::Buffer,
}

impl Deref for WrappedVkBuffer {
    type Target = vk::Buffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl WrappedVkBuffer {
    pub fn new(device: Rc<Device>, size: vk::DeviceSize) -> Result<Self, Error> {
        let buffer_create_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe { device.create_buffer(&buffer_create_info, None) }?;

        Ok(WrappedVkBuffer { device, buffer })
    }

    pub fn get_required_memory_size(&self) -> vk::DeviceSize {
        unsafe { self.device.get_buffer_memory_requirements(self.buffer) }.size
    }
}

impl Drop for WrappedVkBuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_buffer(self.buffer, None);
        }
    }
}

/// Wrap a vk::DeviceMemory so that it auto-deletes when exiting scope.
struct WrappedVkDeviceMemory {
    device: Rc<Device>,
    memory: vk::DeviceMemory,
}

impl Deref for WrappedVkDeviceMemory {
    type Target = vk::DeviceMemory;

    fn deref(&self) -> &Self::Target {
        &self.memory
    }
}

impl WrappedVkDeviceMemory {
    pub fn new(device: Rc<Device>, size: vk::DeviceSize) -> Result<Self, Error> {
        let memory_type_index = unimplemented!();
        let memory_alloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(size)
            .memory_type_index(memory_type_index);
        unsafe {
            let memory = device.allocate_memory(&memory_alloc_info, None)?;
            Ok(WrappedVkDeviceMemory { device, memory })
        }
    }
}

impl Drop for WrappedVkDeviceMemory {
    fn drop(&mut self) {
        unsafe {
            self.device.free_memory(self.memory, None);
        }
    }
}

pub struct MappedDeviceBuffer {
    device: Rc<Device>,
    buffer: WrappedVkBuffer,
    memory: WrappedVkDeviceMemory,
    mapped: *mut c_void,
}

impl Deref for MappedDeviceBuffer {
    type Target = vk::Buffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl MappedDeviceBuffer {
    pub fn new(device: Rc<Device>, size: vk::DeviceSize) -> Result<Self, Error> {
        let buffer = WrappedVkBuffer::new(device.clone(), size)?;
        let required_size = buffer.get_required_memory_size();
        assert!(required_size >= size);
        let memory = WrappedVkDeviceMemory::new(device.clone(), required_size)?;
        unsafe { device.bind_buffer_memory(*buffer, *memory, 0) }?;

        // https://stackoverflow.com/questions/64296581/do-i-need-to-memory-map-unmap-a-buffer-every-time-the-content-of-the-buffer-chan
        let mapped =
            unsafe { device.map_memory(*memory, 0, required_size, vk::MemoryMapFlags::empty()) }?;

        Ok(MappedDeviceBuffer {
            device,
            buffer,
            memory,
            mapped,
        })
    }
}

impl Drop for MappedDeviceBuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.unmap_memory(*self.memory);
        }
    }
}

/// A buffer is composed of multiple device buffers used for multi-buffering (i.e.
/// triple-buffering). These buffers are automatically mapped to system memory to be written to,
/// and unmapped when the object is dropped.
pub struct Buffer {
    binding: u32,
    buffers: Vec<MappedDeviceBuffer>,
}

impl Buffer {
    pub fn new(
        device: &Rc<Device>,
        binding: u32,
        size: vk::DeviceSize,
        num_buffers: usize,
    ) -> Result<Self, Error> {
        debug!("Creating buffer of size {}", size);
        let buffers = (0..num_buffers)
            .map(|_| MappedDeviceBuffer::new(device.clone(), size))
            .collect::<Result<Vec<MappedDeviceBuffer>, Error>>()?;
        Ok(Buffer { binding, buffers })
    }
}
