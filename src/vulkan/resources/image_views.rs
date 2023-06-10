use std::{ops::Deref, rc::Rc, slice::Iter};

use log::debug;

use ash::vk;

use crate::error::Error;

use super::{
    device::Device, image::Image, image_subresource_range::ImageSubresourceRange,
    surface_info::SurfaceInfo,
};

pub struct ImageView {
    device: Rc<Device>,
    image_view: vk::ImageView,
}

impl Deref for ImageView {
    type Target = vk::ImageView;

    fn deref(&self) -> &Self::Target {
        &self.image_view
    }
}

impl ImageView {
    pub unsafe fn new(
        device: &Rc<Device>,
        image: &Image,
        surface_info: &SurfaceInfo,
        image_subresource_range: &ImageSubresourceRange,
    ) -> Result<Rc<Self>, Error> {
        debug!("Creating image views");
        let device = device.clone();
        let format = surface_info.surface_format.format;
        let component_mapping = vk::ComponentMapping::default();

        let create_view_info = vk::ImageViewCreateInfo::builder()
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .components(component_mapping)
            .subresource_range(**image_subresource_range)
            .image(**image);
        let image_view = device.create_image_view(&create_view_info, None)?;

        Ok(Rc::new(ImageView { device, image_view }))
    }

    pub unsafe fn many(
        device: &Rc<Device>,
        images: Iter<impl Deref<Target = Image>>,
        surface_info: &SurfaceInfo,
        image_subresource_range: &ImageSubresourceRange,
    ) -> Result<Vec<Rc<Self>>, Error> {
        images
            .map(|image| ImageView::new(&device, &image, &surface_info, &image_subresource_range))
            .collect()
    }
}

impl Drop for ImageView {
    fn drop(&mut self) {
        debug!("Destroying image views");
        unsafe {
            self.device.destroy_image_view(**self, None);
        };
    }
}
