use std::{path::Path, rc::Rc};

use ash::vk;
use filetime::FileTime;
use log::{debug, error, info, warn};
use winit::event_loop::ControlFlow;

use crate::{
    error::Error,
    utils::mtime,
    window::{App, Window},
};

pub mod multi_buffer;
pub mod resources;

use self::{
    multi_buffer::MultiBuffer,
    resources::{
        command_buffer::CommandBuffer, command_pool::CommandPool, compute_queue::ComputeQueue,
        descriptor_pool::DescriptorPool, descriptor_set_layout::DescriptorSetLayout,
        descriptor_set_layout_bindings::DescriptorSetLayoutBindings,
        descriptor_sets::DescriptorSets, device::Device, entry::Entry, fence::Fence,
        image_subresource_range::ImageSubresourceRange, image_views::ImageViews, images::Images,
        instance::Instance, physical_device::PhysicalDevice, pipeline::Pipeline,
        pipeline_layout::PipelineLayout, sampler::Sampler, semaphore::Semaphore,
        shader_module::ShaderModule, surface::Surface, surface_info::SurfaceInfo,
        surface_loader::SurfaceLoader, swapchain::Swapchain, swapchain_loader::SwapchainLoader,
    },
};

// This struct is not supposed to be read on the CPU, it only maps the structure of the push
// constants block so that it can be written to the gpu memory properly.
#[allow(dead_code)]
struct PushConstants {
    num_frames: u32,
}

// Define fields in reverse drop order.
pub struct Vulkan {
    pub num_frames: u32,
    pub binding_index: usize,
    stale_image_layout: bool,
    stale_swapchain: bool,

    image_acquired_semaphore: Rc<Semaphore>,
    compute_complete_semaphore: Rc<Semaphore>,
    reuse_command_buffer_fence: Rc<Fence>,

    pipelines: Vec<Rc<Pipeline>>,

    pipeline_layout: Rc<PipelineLayout<PushConstants>>,

    descriptor_sets_sets: Rc<Vec<DescriptorSets>>,
    _descriptor_pool: Rc<DescriptorPool>,
    _descriptor_set_layouts: Rc<Vec<DescriptorSetLayout>>,
    _descriptor_set_layout_binding_sets: Rc<Vec<DescriptorSetLayoutBindings>>,

    compute_shader_modules: Vec<Rc<ShaderModule>>,
    compute_shader_module_mtimes: Vec<FileTime>,

    sampler: Rc<Sampler>,
    present_name: String,
    image_views: Rc<ImageViews>,
    images: Rc<Images>,
    image_subresource_range: ImageSubresourceRange,
    swapchain: Rc<Swapchain>,
    swapchain_loader: Rc<SwapchainLoader>,
    surface_info: SurfaceInfo,
    window_size: (u32, u32),
    command_buffer: Rc<CommandBuffer>,
    _command_pool: Rc<CommandPool>,
    compute_queue: Rc<ComputeQueue>,

    device: Rc<Device>,
    physical_device: Rc<PhysicalDevice>,
    surface: Rc<Surface>,
    surface_loader: Rc<SurfaceLoader>,

    _instance: Rc<Instance>,
    _entry: Rc<Entry>,
}

impl Vulkan {
    unsafe fn write_descriptor_sets(
        &self,
        image_descriptors: &[(
            vk::DescriptorType,
            [vk::DescriptorImageInfo; 1],
            vk::DescriptorSet,
            u32,
        )],
        buffer_descriptors: &[(
            vk::DescriptorType,
            [vk::DescriptorBufferInfo; 1],
            vk::DescriptorSet,
            u32,
        )],
    ) {
        let image_descriptors = image_descriptors.iter().map(
            |&(descriptor_type, ref image_info, descriptor_set, descriptor_binding)| {
                vk::WriteDescriptorSet::builder()
                    .descriptor_type(descriptor_type)
                    .image_info(image_info)
                    .dst_set(descriptor_set)
                    .dst_binding(descriptor_binding)
                    .dst_array_element(0)
                    .build()
            },
        );

        let buffer_descriptors = buffer_descriptors.iter().map(
            |&(descriptor_type, ref buffer_info, descriptor_set, descriptor_binding)| {
                vk::WriteDescriptorSet::builder()
                    .descriptor_type(descriptor_type)
                    .buffer_info(buffer_info)
                    .dst_set(descriptor_set)
                    .dst_binding(descriptor_binding)
                    .dst_array_element(0)
                    .build()
            },
        );

        let write_descriptor_sets: Vec<vk::WriteDescriptorSet> =
            image_descriptors.chain(buffer_descriptors).collect();

        self.device
            .update_descriptor_sets(&write_descriptor_sets, &[]);
    }

    unsafe fn initialize_descriptor_sets(&self) {
        debug!("Initializing present descriptor sets");

        let name = &self.present_name;
        let present_declaration = self
            .compute_shader_modules
            .iter()
            .find_map(|module| module.variable_declaration(name))
            .expect("Present image not found in shader modules.");

        let present_binding = present_declaration
            .binding
            .expect("Present image has bo binding");
        let present_set = present_declaration.checked_set();
        let present_descriptor_sets = &self.descriptor_sets_sets[present_set];

        let image_descriptors = self
            .image_views
            .iter()
            .zip(present_descriptor_sets.iter())
            .map(|(image_view, descriptor_set)| {
                let image_info = vk::DescriptorImageInfo::builder()
                    .image_view(*image_view)
                    .sampler(**self.sampler)
                    .image_layout(vk::ImageLayout::GENERAL)
                    .build();

                (
                    vk::DescriptorType::STORAGE_IMAGE,
                    [image_info],
                    *descriptor_set,
                    present_binding,
                )
            })
            .collect::<Vec<_>>();

        self.write_descriptor_sets(&image_descriptors, &[]);
    }

    pub fn new(window: &Window, compute_shader_paths: &[&Path]) -> Result<Self, Error> {
        debug!("Initializing video system");
        unsafe {
            let entry = Entry::new()?;
            let instance = Instance::new(window, &entry)?;
            let surface_loader = SurfaceLoader::new(&entry, &instance)?;
            let surface = Surface::new(window, &entry, &instance, &surface_loader)?;
            let physical_device = PhysicalDevice::new(&instance, &surface)?;
            let device = Device::new(&instance, &physical_device)?;
            let compute_queue = ComputeQueue::new(&physical_device, &device)?;
            let command_pool = CommandPool::new(&physical_device, &device)?;
            let command_buffer = CommandBuffer::new(&device, &command_pool)?;

            let window_size = (window.width, window.height);
            let surface_info =
                SurfaceInfo::new(window_size, &physical_device, &surface_loader, &surface)?;
            let swapchain_loader = SwapchainLoader::new(&instance, &device)?;
            let swapchain = Swapchain::new(&surface, &surface_info, &swapchain_loader, None)?;
            let image_subresource_range = ImageSubresourceRange::new()?;
            let images = Images::new(&swapchain_loader, &swapchain)?;
            let image_views =
                ImageViews::new(&device, &images, &surface_info, &image_subresource_range)?;
            let present_name = "present".to_owned();
            let sampler = Sampler::new(&device)?;

            let mut compute_shader_module_mtimes = Vec::new();
            let mut compute_shader_modules = Vec::new();
            for compute_shader_path in compute_shader_paths {
                compute_shader_module_mtimes.push(mtime(compute_shader_path)?);
                compute_shader_modules.push(ShaderModule::new(&device, compute_shader_path)?);
            }

            let descriptor_set_layout_binding_sets =
                DescriptorSetLayoutBindings::new(&compute_shader_modules)?;
            let descriptor_set_layouts =
                DescriptorSetLayout::new(&device, &descriptor_set_layout_binding_sets)?;
            let descriptor_pool = DescriptorPool::new(
                &device,
                &descriptor_set_layout_binding_sets,
                surface_info.desired_image_count,
            )?;
            let descriptor_sets_sets = DescriptorSets::new(
                &device,
                &descriptor_set_layouts,
                &descriptor_pool,
                surface_info.desired_image_count,
            )?;

            let pipeline_layout = PipelineLayout::new(&device, &descriptor_set_layouts)?;

            let mut pipelines = Vec::new();
            for compute_shader_module in &compute_shader_modules {
                pipelines.push(Pipeline::new(
                    &device,
                    &compute_shader_module,
                    &pipeline_layout,
                )?);
            }

            let reuse_command_buffer_fence = Fence::new(&device)?;
            let image_acquired_semaphore = Semaphore::new(&device)?;
            let compute_complete_semaphore = Semaphore::new(&device)?;

            let vulkan = Vulkan {
                _entry: entry,
                _instance: instance,
                surface_loader,
                surface,
                physical_device,
                device,
                compute_queue,
                _command_pool: command_pool,
                command_buffer,
                window_size,
                surface_info,
                swapchain_loader,
                swapchain,
                image_subresource_range,
                images,
                image_views,
                present_name,
                sampler,
                compute_shader_module_mtimes,
                compute_shader_modules,
                _descriptor_set_layout_binding_sets: descriptor_set_layout_binding_sets,
                _descriptor_set_layouts: descriptor_set_layouts,
                _descriptor_pool: descriptor_pool,
                descriptor_sets_sets,
                pipeline_layout,
                pipelines,
                reuse_command_buffer_fence,
                image_acquired_semaphore,
                compute_complete_semaphore,
                binding_index: 0,
                num_frames: 0,
                // Images are in undefined layout when created.
                stale_image_layout: true,
                stale_swapchain: false,
            };

            vulkan.initialize_descriptor_sets();

            Ok(vulkan)
        }
    }

    unsafe fn recompile_shader_if_modified(&mut self) -> Result<(), Error> {
        for index in 0..self.compute_shader_modules.len() {
            let module = &self.compute_shader_modules[index];
            let module_mtime = self.compute_shader_module_mtimes[index];

            if mtime(&module.source_path)? > module_mtime {
                self.compute_shader_module_mtimes[index] = mtime(&module.source_path)?;
                info!("Shader source modified, recompiling...");
                self.wait_idle();

                match module.rebuild() {
                    Ok(new_module) => {
                        let new_pipeline =
                            Pipeline::new(&self.device, &new_module, &self.pipeline_layout)?;

                        self.compute_shader_modules[index] = new_module;
                        self.pipelines[index] = new_pipeline;

                        self.initialize_descriptor_sets();
                    }
                    Err(err) => error!("{}", err),
                }
            }
        }
        Ok(())
    }

    pub unsafe fn reinitialize_after_resize(&mut self) -> Result<(), Error> {
        info!(
            "Reinitializing surface after resize to {:?}",
            self.window_size
        );

        self.wait_idle();

        // TODO
        // The following code first creates the new resources, replaces them in `self` and only
        // then frees/drops the old ones. In case we are at memory limits this might leat to GPU
        // OOM errors. Alternative solutions: wrap all fields in `Option` or separate between free
        // and `drop`.

        self.surface_info = SurfaceInfo::new(
            self.window_size,
            &self.physical_device,
            &self.surface_loader,
            &self.surface,
        )?;
        self.swapchain = Swapchain::new(
            &self.surface,
            &self.surface_info,
            &self.swapchain_loader,
            Some(**self.swapchain),
        )?;
        self.images = Images::new(&self.swapchain_loader, &self.swapchain)?;
        self.image_views = ImageViews::new(
            &self.device,
            &self.images,
            &self.surface_info,
            &self.image_subresource_range,
        )?;

        self.initialize_descriptor_sets();

        Ok(())
    }

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
                .buffers
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

    unsafe fn begin_command_buffer(&self) -> Result<(), Error> {
        let command_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        Ok(self
            .device
            .begin_command_buffer(**self.command_buffer, &command_buffer_begin_info)?)
    }

    unsafe fn end_command_buffer(&self) -> Result<(), Error> {
        Ok(self.device.end_command_buffer(**self.command_buffer)?)
    }

    unsafe fn queue_submit(
        &self,
        wait_semaphores: &[vk::Semaphore],
        wait_semaphore_stages: &[vk::PipelineStageFlags],
        signal_semaphores: &[vk::Semaphore],
    ) -> Result<(), Error> {
        let submit_info = vk::SubmitInfo::builder()
            .command_buffers(&[**self.command_buffer])
            .wait_semaphores(wait_semaphores)
            .wait_dst_stage_mask(wait_semaphore_stages)
            .signal_semaphores(signal_semaphores)
            .build();

        Ok(self.device.queue_submit(
            **self.compute_queue,
            &[submit_info],
            **self.reuse_command_buffer_fence,
        )?)
    }

    unsafe fn queue_submit_task(&self) -> Result<(), Error> {
        self.queue_submit(&[], &[], &[])
    }

    unsafe fn queue_submit_compute(&self) -> Result<(), Error> {
        self.queue_submit(
            &[**self.image_acquired_semaphore],
            &[vk::PipelineStageFlags::COMPUTE_SHADER],
            &[**self.compute_complete_semaphore],
        )
    }

    unsafe fn bind_pipeline(&self, pipeline: &Pipeline) {
        self.device.cmd_bind_pipeline(
            **self.command_buffer,
            vk::PipelineBindPoint::COMPUTE,
            **pipeline,
        );
    }

    // Requires bound pipeline and a started command buffer.
    unsafe fn image_memory_barrier_layout_transition(
        &self,
        image: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) {
        let memory_barrier = vk::ImageMemoryBarrier::builder()
            .image(image)
            .subresource_range(*self.image_subresource_range)
            .old_layout(old_layout)
            .new_layout(new_layout)
            .build();
        let memory_barriers = [memory_barrier];

        self.device.cmd_pipeline_barrier(
            **self.command_buffer,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::BY_REGION,
            &[],
            &[],
            &memory_barriers,
        );
    }

    unsafe fn transition_images_to_present(&self) -> Result<(), Error> {
        debug!("Transitioning image layout from `UNDEFINED` to `PRESENT_SRC`");

        self.reuse_command_buffer_fence.wait()?;
        self.reuse_command_buffer_fence.reset()?;

        self.begin_command_buffer()?;
        for image in self.images.iter() {
            self.image_memory_barrier_layout_transition(
                *image,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::PRESENT_SRC_KHR,
            );
        }
        self.end_command_buffer()?;

        self.queue_submit_task()
    }

    unsafe fn acquire_next_image(&self) -> Result<(usize, vk::Image), Error> {
        let (present_index, _) = self.swapchain_loader.acquire_next_image(
            **self.swapchain,
            std::u64::MAX,
            **self.image_acquired_semaphore,
            vk::Fence::null(),
        )?;

        Ok((present_index as usize, self.images[present_index as usize]))
    }

    unsafe fn push_constants(&self, push_constants: &PushConstants) {
        unsafe fn as_u8_slice<T: Sized>(p: &T) -> &[u8] {
            ::std::slice::from_raw_parts((p as *const T) as *const u8, ::std::mem::size_of::<T>())
        }

        let constants = as_u8_slice(push_constants);
        self.device.cmd_push_constants(
            **self.command_buffer,
            **self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            constants,
        );
    }

    unsafe fn bind_descriptor_set(&self, descriptor_set_indices: &[usize]) {
        let descriptor_sets = self
            .descriptor_sets_sets
            .iter()
            .zip(descriptor_set_indices.iter())
            .map(|(descriptor_set, &index_within_set)| descriptor_set[index_within_set])
            .collect::<Vec<_>>();

        self.device.cmd_bind_descriptor_sets(
            **self.command_buffer,
            vk::PipelineBindPoint::COMPUTE,
            **self.pipeline_layout,
            0,
            &descriptor_sets,
            &[],
        );
    }

    unsafe fn dispatch(&self, compute_shader_module: &ShaderModule) {
        let local_size = compute_shader_module.local_size;
        let window_size = self.window_size;
        let invocation_x = window_size.0 / local_size.0;
        let invocation_y = window_size.1 / local_size.1;
        let invocation_z = 1; // Hardcode for now.

        self.device.cmd_dispatch(
            **self.command_buffer,
            invocation_x,
            invocation_y,
            invocation_z,
        );
    }

    unsafe fn present(&self, present_index: usize) -> Result<(), Error> {
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&[**self.compute_complete_semaphore])
            .swapchains(&[**self.swapchain])
            .image_indices(&[present_index as u32])
            .build();

        Ok(self
            .swapchain_loader
            .queue_present(**self.compute_queue, &present_info)
            .map(|suboptimal| {
                if suboptimal {
                    warn!("Swapchain is suboptimal");
                }
            })?)
    }

    unsafe fn render_next_frame(&mut self) -> Result<(), Error> {
        let (present_index, present_image) = self.acquire_next_image()?;

        self.reuse_command_buffer_fence.wait()?;
        self.reuse_command_buffer_fence.reset()?;

        self.begin_command_buffer()?;

        // Transition image to "GENERAL" layout.
        self.image_memory_barrier_layout_transition(
            present_image,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::ImageLayout::GENERAL,
        );

        let push_constants = PushConstants {
            num_frames: self.num_frames,
        };
        self.push_constants(&push_constants);
        self.bind_descriptor_set(&[present_index, self.binding_index]);

        let iter = self
            .pipelines
            .iter()
            .zip(self.compute_shader_modules.iter());
        for (pipeline, shader_module) in iter {
            self.bind_pipeline(pipeline);
            self.dispatch(shader_module);
        }

        // Transition image to the "PRESENT_SRC" layout for presentation.
        self.image_memory_barrier_layout_transition(
            present_image,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );

        self.end_command_buffer()?;
        self.queue_submit_compute()?;

        // Present as soon as `compute_complete_semaphore` trips.
        let present_result = self.present(present_index);
        if let Err(Error::Vk(vk::Result::ERROR_OUT_OF_DATE_KHR)) = present_result {
            debug!("Marking swapchain as stale because window was resized.");
            self.stale_swapchain = true;
        }

        self.num_frames += 1;
        self.binding_index =
            (self.binding_index + 1) % self.surface_info.desired_image_count as usize;

        Ok(())
    }

    pub fn wait_idle(&self) {
        unsafe {
            self.device
                .device_wait_idle()
                .expect("Failed to wait for device idle");
        }
    }
}

impl App for Vulkan {
    // TODO Remove unwraps
    fn run_frame(&mut self) -> ControlFlow {
        unsafe {
            if self.stale_swapchain {
                if let Err(error) = self.reinitialize_after_resize() {
                    error!("Failed to reinitialize pipeline after resize: {:?}", error);
                    return ControlFlow::ExitWithCode(1);
                }
                self.stale_image_layout = true;
                self.stale_swapchain = false;
            }

            // Initially and after a resize, the image layout is stale.
            if self.stale_image_layout {
                self.transition_images_to_present().unwrap();
                self.stale_image_layout = false;
            }

            self.recompile_shader_if_modified().unwrap();
            self.render_next_frame().unwrap();
            ControlFlow::Poll
        }
    }

    fn handle_resize(&mut self, new_size: (u32, u32)) -> Result<(), Error> {
        self.window_size = new_size;
        self.stale_swapchain = true;
        Ok(())
    }
}
