use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::{self, vk};

use crate::error::Error;

use super::{device::Device, physical_device::PhysicalDevice};

pub struct ComputeQueue {
    compute_queue: vk::Queue,
}

impl Deref for ComputeQueue {
    type Target = vk::Queue;

    fn deref(&self) -> &Self::Target {
        &self.compute_queue
    }
}

impl ComputeQueue {
    pub unsafe fn new(
        physical_device: &PhysicalDevice,
        device: &Device,
    ) -> Result<Rc<Self>, Error> {
        debug!("Creating compute queue");
        let compute_queue = device.get_device_queue(physical_device.compute_queue_family_index, 0);

        Ok(Rc::new(ComputeQueue { compute_queue }))
    }
}
