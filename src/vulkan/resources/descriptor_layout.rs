use std::{ops::Deref, rc::Rc};

use tracing::debug;

use ash::vk;

use crate::{error::Error, vulkan::resources::descriptors::DescriptorBinding};

use super::{descriptors::Descriptors, device::Device};

pub struct DescriptorLayout {
    device: Rc<Device>,
    layout: vk::DescriptorSetLayout,
}

impl Deref for DescriptorLayout {
    type Target = vk::DescriptorSetLayout;

    fn deref(&self) -> &Self::Target {
        &self.layout
    }
}

impl DescriptorLayout {
    pub unsafe fn new(device: &Rc<Device>, descriptors: &Descriptors) -> Result<Rc<Self>, Error> {
        debug!("Creating descriptor layouts");

        let device = device.clone();
        let bindings = descriptors
            .iter()
            .map(DescriptorBinding::as_descriptor_set_layout_binding)
            .collect::<Vec<_>>();
        let descriptor_layout_create_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR)
            .bindings(&bindings);
        let layout = device.create_descriptor_set_layout(&descriptor_layout_create_info, None)?;

        Ok(Rc::new(DescriptorLayout { device, layout }))
    }
}

impl Drop for DescriptorLayout {
    fn drop(&mut self) {
        debug!("Dropping descriptor set layout");
        unsafe {
            self.device.destroy_descriptor_set_layout(**self, None);
        }
    }
}
