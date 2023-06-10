use std::{ops::Deref, rc::Rc};

use ash::{self, vk};
use log::debug;

use crate::error::Error;

use super::{
    resources::{
        device::Device, device_memory::DeviceMemory, image::Image,
        image_subresource_range::ImageSubresourceRange, image_views::ImageView,
        physical_device::PhysicalDevice, surface_info::SurfaceInfo,
    },
    Vulkan,
};

pub struct MultiImageUnit {
    pub image: Rc<Image>,
    pub memory: Rc<DeviceMemory>,
    pub view: Rc<ImageView>,
}

impl MultiImageUnit {
    pub unsafe fn new(
        physical_device: &PhysicalDevice,
        device: &Rc<Device>,
        surface_info: &SurfaceInfo,
        image_subresource_range: &ImageSubresourceRange,
    ) -> Result<Self, Error> {
        let image = Image::new(device, surface_info)?;
        let memory = DeviceMemory::new(physical_device, device, image.get_required_memory_size())?;

        device.bind_image_memory(**image, **memory, 0)?;

        let view = ImageView::new(device, &*image, surface_info, image_subresource_range)?;

        Ok(MultiImageUnit {
            image,
            memory,
            view,
        })
    }
}

pub struct MultiImage(Vec<MultiImageUnit>);

impl Deref for MultiImage {
    type Target = [MultiImageUnit];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl MultiImage {
    pub unsafe fn new(
        physical_device: &Rc<PhysicalDevice>,
        device: &Rc<Device>,
        surface_info: &SurfaceInfo,
        image_subresource_range: &ImageSubresourceRange,
        num_images: usize,
    ) -> Result<Rc<Self>, Error> {
        debug!(
            "Creating image of size {:?}",
            surface_info.surface_resolution
        );
        let images = (0..num_images)
            .map(|_| {
                MultiImageUnit::new(
                    physical_device,
                    device,
                    surface_info,
                    image_subresource_range,
                )
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(Rc::new(MultiImage(images)))
    }
}

impl Drop for MultiImage {
    fn drop(&mut self) {
        debug!("Destroying buffer");
    }
}

impl Vulkan {
    pub fn new_multi_image(&self, name: &str) -> Result<Rc<MultiImage>, Error> {
        // TODO num buffers? What does this mean? ?????????
        let num_images = self.surface_info.desired_image_count as usize;
        unsafe {
            let declaration = self
                .compute_shader_modules
                .iter()
                .find_map(|module| module.variable_declaration(name))
                .ok_or_else(|| {
                    let msg = format!("No variable '{name}' within shader module.");
                    Error::Local(msg)
                })?;
            let binding = declaration.binding.ok_or_else(|| {
                let msg = format!("Variable '{name}' does not specify a binding.");
                Error::Local(msg)
            })?;
            let set = declaration.checked_set();

            let image = MultiImage::new(
                &self.physical_device,
                &self.device,
                &self.surface_info,
                &self.image_subresource_range,
                num_images,
            )?;

            let descriptor_sets = &self.descriptor_sets_sets[set];
            let image_descriptors: Vec<(
                vk::DescriptorType,
                [vk::DescriptorImageInfo; 1],
                vk::DescriptorSet,
                u32,
            )> = image
                .iter()
                .zip(descriptor_sets.iter())
                .map(|(image_unit, descriptor_set)| {
                    let image_info = vk::DescriptorImageInfo::builder()
                        .image_view(**image_unit.view)
                        .sampler(**self.sampler)
                        .image_layout(vk::ImageLayout::GENERAL)
                        .build();

                    (vk::DescriptorType::STORAGE_IMAGE, [image_info], *descriptor_set, binding)
                })
                .collect();

            self.write_descriptor_sets(&image_descriptors, &[]);

            Ok(image)
        }
    }
}