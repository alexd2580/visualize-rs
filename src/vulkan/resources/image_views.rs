use std::{ops::Deref, rc::Rc};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::{
    device::Device, image_subresource_range::ImageSubresourceRange, images::Images,
    surface_info::SurfaceInfo,
};

pub struct ImageViews {
    device: Rc<Device>,
    image_views: Vec<vk::ImageView>,
}

impl Deref for ImageViews {
    type Target = [vk::ImageView];

    fn deref(&self) -> &Self::Target {
        &self.image_views
    }
}

impl ImageViews {
    pub unsafe fn new(
        device: &Rc<Device>,
        images: &Images,
        surface_info: &SurfaceInfo,
        image_subresource_range: &ImageSubresourceRange,
    ) -> Result<Rc<Self>, Error> {
        debug!("Creating image views");
        let device = device.clone();
        let format = surface_info.surface_format.format;
        let component_mapping = vk::ComponentMapping::default();

        let image_views = images
            .iter()
            .map(|&image| {
                let create_view_info = vk::ImageViewCreateInfo::builder()
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .components(component_mapping)
                    .subresource_range(**image_subresource_range)
                    .image(image);
                device.create_image_view(&create_view_info, None).unwrap() // TODO
            })
            .collect();

        Ok(Rc::new(ImageViews {
            device,
            image_views,
        }))
    }
}

impl Drop for ImageViews {
    fn drop(&mut self) {
        debug!("Destroying image views");
        unsafe {
            self.iter()
                .for_each(|&view| self.device.destroy_image_view(view, None));
        };
    }
}
