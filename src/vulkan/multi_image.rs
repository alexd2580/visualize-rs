use std::{ops::Deref, rc::Rc};

use ash::vk;
use log::debug;

use crate::error::Error;

use super::{
    resources::{
        device::Device, device_memory::DeviceMemory, image::Image,
        image_subresource_range::ImageSubresourceRange, image_view::ImageView,
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
        let required_memory_size = image.get_required_memory_size().unwrap();
        let memory = DeviceMemory::new(
            physical_device.image_memory_type_index,
            device,
            required_memory_size,
        )?;

        device.bind_image_memory(**image, **memory, 0)?;

        let view = ImageView::new(device, &image, surface_info, image_subresource_range)?;

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
    pub fn new_multi_image(&mut self, name: &str) -> Result<Rc<MultiImage>, Error> {
        // TODO num buffers? What does this mean? ?????????
        let num_images = self.surface_info.desired_image_count as usize;
        unsafe {
            let image = MultiImage::new(
                &self.physical_device,
                &self.device,
                &self.surface_info,
                &self.image_subresource_range,
                num_images,
            )?;

            for image_unit in image.iter() {
                self.stale_images.push((
                    image_unit.image.clone(),
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::GENERAL,
                ));
            }

            let views_and_samplers = image
                .iter()
                .map(|unit| (unit.view.clone(), self.sampler.clone()))
                .collect();
            self.image_binding_updates
                .push((name.to_owned(), views_and_samplers));

            Ok(image)
        }
    }
}
