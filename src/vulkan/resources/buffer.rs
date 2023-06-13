use std::{ops::Deref, rc::Rc};

use ash::{self, vk};

use crate::error::Error;

use super::device::Device;

pub struct Buffer {
    pub size: vk::DeviceSize,
    device: Rc<Device>,
    buffer: vk::Buffer,
}

impl Deref for Buffer {
    type Target = vk::Buffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl Buffer {
    pub unsafe fn new(device: &Rc<Device>, size: vk::DeviceSize) -> Result<Rc<Self>, Error> {
        let device = device.clone();
        let buffer_create_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = device.create_buffer(&buffer_create_info, None)?;

        Ok(Rc::new(Buffer {
            size,
            device,
            buffer,
        }))
    }

    pub unsafe fn get_required_memory_size(&self) -> vk::DeviceSize {
        self.device.get_buffer_memory_requirements(**self).size
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_buffer(self.buffer, None);
        }
    }
}
