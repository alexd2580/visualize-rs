use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::device::Device;

pub struct Sampler {
    device: Rc<Device>,
    sampler: vk::Sampler,
}

impl Deref for Sampler {
    type Target = vk::Sampler;

    fn deref(&self) -> &Self::Target {
        &self.sampler
    }
}

impl Sampler {
    pub unsafe fn new(device: &Rc<Device>) -> Result<Rc<Self>, Error> {
        debug!("Creating sampler");
        let device = device.clone();
        let sampler_create_info = vk::SamplerCreateInfo {
            mag_filter: vk::Filter::NEAREST,
            min_filter: vk::Filter::NEAREST,
            mipmap_mode: vk::SamplerMipmapMode::NEAREST,
            address_mode_u: vk::SamplerAddressMode::CLAMP_TO_EDGE,
            address_mode_v: vk::SamplerAddressMode::CLAMP_TO_EDGE,
            address_mode_w: vk::SamplerAddressMode::CLAMP_TO_EDGE,
            max_anisotropy: 0.0,
            border_color: vk::BorderColor::FLOAT_OPAQUE_WHITE,
            compare_op: vk::CompareOp::NEVER,
            ..Default::default()
        };
        let sampler = device.create_sampler(&sampler_create_info, None)?;

        Ok(Rc::new(Sampler { device, sampler }))
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        debug!("Destroying sampler");
        unsafe {
            self.device.destroy_sampler(self.sampler, None);
        }
    }
}
