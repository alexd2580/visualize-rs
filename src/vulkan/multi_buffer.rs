use std::{ffi::c_void, ops::Deref, rc::Rc};

use ash::{self, vk};
use log::debug;

use crate::error::Error;

use super::{
    resources::{
        buffer::Buffer, device::Device, device_memory::DeviceMemory, memory_mapping::MemoryMapping,
        physical_device::PhysicalDevice,
    },
    Vulkan,
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
pub struct MultiBuffer(Vec<MultiBufferUnit>);

impl Deref for MultiBuffer {
    type Target = [MultiBufferUnit];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
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
        Ok(Rc::new(MultiBuffer(buffers)))
    }

    pub fn mapped(&self, index: usize) -> *mut c_void {
        **self[index].mapping
    }
}

impl Drop for MultiBuffer {
    fn drop(&mut self) {
        debug!("Destroying buffer");
    }
}

impl Vulkan {
    pub fn new_multi_buffer(
        &self,
        name: &str,
        size: vk::DeviceSize,
    ) -> Result<Rc<MultiBuffer>, Error> {
        let num_buffers = self.surface_info.desired_image_count as usize;
        // TODO num buffers? What does this mean?
        unsafe {
            let declaration = self
                .compute_shader_modules
                .iter()
                .find_map(|module| module.block_declaration(name))
                .ok_or_else(|| {
                    let msg = format!("No block '{name}' within shader module.");
                    Error::Local(msg)
                })?;
            let storage = declaration.storage;
            let binding = declaration.binding.ok_or_else(|| {
                let msg = format!("Block '{name}' does not specify a binding.");
                Error::Local(msg)
            })?;
            let set = declaration.checked_set();

            let buffer = MultiBuffer::new(&self.physical_device, &self.device, size, num_buffers)?;

            let descriptor_sets = &self.descriptor_sets_sets[set];
            let buffer_descriptors: Vec<(
                vk::DescriptorType,
                [vk::DescriptorBufferInfo; 1],
                vk::DescriptorSet,
                u32,
            )> = buffer
                .iter()
                .zip(descriptor_sets.iter())
                .map(|(buffer_unit, descriptor_set)| {
                    let buffer_info = vk::DescriptorBufferInfo::builder()
                        .buffer(**buffer_unit.buffer)
                        .offset(0)
                        .range(size)
                        .build();

                    (storage, [buffer_info], *descriptor_set, binding)
                })
                .collect();

            self.write_descriptor_sets(&[], &buffer_descriptors);

            Ok(buffer)
        }
    }
}
