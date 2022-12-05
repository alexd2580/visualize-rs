use std::rc::Rc;

use ash::{extensions, vk};

use crate::error::Error;
use log::warn;

use super::{
    device::Device, instance::Instance, semaphore::Semaphore, surface::Surface,
    surface_info::SurfaceInfo,
};

pub struct Swapchain {
    device: Rc<Device>,
    swapchain_loader: extensions::khr::Swapchain,
    pub swapchain: vk::SwapchainKHR,
    pub image_subresource_range: vk::ImageSubresourceRange,
    pub images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
    sampler: vk::Sampler,
}

impl Swapchain {
    pub fn new(
        instance: &Instance,
        device: Rc<Device>,
        surface: &Surface,
        surface_info: &SurfaceInfo,
        old_swapchain: Option<vk::SwapchainKHR>,
    ) -> Result<Swapchain, Error> {
        let surface_format = &surface_info.surface_format;

        let swapchain_loader = extensions::khr::Swapchain::new(&instance.instance, &device);
        let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(surface.surface)
            .min_image_count(surface_info.desired_image_count)
            .image_color_space(surface_format.color_space)
            .image_format(surface_format.format)
            .image_extent(surface_info.surface_resolution)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::STORAGE)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(vk::SurfaceTransformFlagsKHR::IDENTITY)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(surface_info.desired_present_mode)
            .clipped(true)
            .image_array_layers(1)
            .old_swapchain(old_swapchain.unwrap_or(vk::SwapchainKHR::null()));

        let swapchain = unsafe { swapchain_loader.create_swapchain(&swapchain_create_info, None) }
            .map_err(Error::Vk)?;

        let image_subresource_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };

        let images =
            unsafe { swapchain_loader.get_swapchain_images(swapchain) }.map_err(Error::Vk)?;

        let image_views = images
            .iter()
            .map(|&image| {
                let component_mapping = vk::ComponentMapping::default();
                let create_view_info = vk::ImageViewCreateInfo::builder()
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(surface_info.surface_format.format)
                    .components(component_mapping)
                    .subresource_range(image_subresource_range)
                    .image(image);
                unsafe { device.create_image_view(&create_view_info, None) }.unwrap()
            })
            .collect();

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
        let sampler =
            unsafe { device.create_sampler(&sampler_create_info, None) }.map_err(Error::Vk)?;

        Ok(Swapchain {
            device,
            swapchain_loader,
            swapchain,
            image_subresource_range,
            images,
            image_views,
            sampler,
        })
    }

    pub fn initialize_descriptor_sets(&self, descriptor_sets: &[vk::DescriptorSet]) {
        // Can't merge maps, we need to have an adressable list of descimageinfos.
        // For reference see `WriteDescriptorSetBuilder::image_info`.
        let image_infos: Vec<[vk::DescriptorImageInfo; 1]> = self
            .image_views
            .iter()
            .map(|&image_view| {
                [vk::DescriptorImageInfo::builder()
                    .image_view(image_view)
                    .sampler(self.sampler)
                    .image_layout(vk::ImageLayout::GENERAL)
                    .build()]
            })
            .collect();

        let write_descriptor_sets: Vec<vk::WriteDescriptorSet> = descriptor_sets
            .iter()
            .zip(image_infos.iter())
            .map(|(&descriptor_set, image_info)| {
                vk::WriteDescriptorSet::builder()
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(image_info)
                    .dst_set(descriptor_set)
                    .dst_binding(0)
                    .dst_array_element(0)
                    .build()
            })
            .collect();

        unsafe {
            self.device
                .update_descriptor_sets(&write_descriptor_sets, &[])
        };
    }

    pub fn acquire_next_image(&self, signal_semaphore: vk::Semaphore) -> (usize, vk::Image) {
        let (present_index, _) = unsafe {
            self.swapchain_loader.acquire_next_image(
                self.swapchain,
                std::u64::MAX,
                signal_semaphore,
                vk::Fence::null(),
            )
        }
        .expect("Failed to acquire next image");

        (present_index as usize, self.images[present_index as usize])
    }

    pub fn present(
        &self,
        queue: vk::Queue,
        present_index: usize,
        wait_semaphore: &Semaphore,
    ) -> Result<(), Error> {
        let wait_semaphores = [**wait_semaphore];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphores)
            .swapchains(&[self.swapchain])
            .image_indices(&[present_index as u32])
            .build();

        unsafe { self.swapchain_loader.queue_present(queue, &present_info) }
            .map(|suboptimal| {
                if suboptimal {
                    warn!("Swapchain is suboptimal");
                }
            })
            .map_err(Error::Vk)
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_sampler(self.sampler, None);
            self.image_views
                .iter()
                .for_each(|&view| self.device.destroy_image_view(view, None));
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None)
        };
    }
}
