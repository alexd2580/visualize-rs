use std::{collections::HashMap, mem, ops::Deref, path::Path, rc::Rc};

use ash::vk;
use filetime::FileTime;
use log::{debug, error, info, warn};
use winit::event_loop::ControlFlow;

use crate::{
    error::Error,
    utils::mtime,
    window::{App, Window},
};

mod flush_binding_updates;
pub mod multi_buffer;
pub mod multi_image;
pub mod resources;

use self::resources::{
    buffer::Buffer, command_buffer::CommandBuffer, command_pool::CommandPool,
    compute_queue::ComputeQueue, descriptor_bindings::DescriptorBindings,
    descriptor_layouts::DescriptorLayouts, descriptor_pool::DescriptorPool,
    descriptor_sets::DescriptorSets, device::Device, entry::Entry, fence::Fence, image::Image,
    image_subresource_range::ImageSubresourceRange, image_view::ImageView, instance::Instance,
    physical_device::PhysicalDevice, pipeline::Pipeline, pipeline_layout::PipelineLayout,
    sampler::Sampler, semaphore::Semaphore, shader_module::ShaderModule, surface::Surface,
    surface_info::SurfaceInfo, surface_loader::SurfaceLoader, swapchain::Swapchain,
    swapchain_loader::SwapchainLoader,
};

type ImageBindingUpdate = (String, Vec<(Rc<ImageView>, Rc<Sampler>)>);
type BufferBindingUpdate = (String, Vec<Rc<Buffer>>);

enum Value {
    U32(u32),
}

struct ShaderResources {
    // Pipelines.
    pipeline: Rc<Pipeline>,
    pipeline_layout: Rc<PipelineLayout>,

    // Descriptors.
    descriptor_sets: Rc<DescriptorSets>,
    descriptor_pool: Rc<DescriptorPool>,
    descriptor_layouts: Rc<DescriptorLayouts>,
    descriptor_bindings: Rc<DescriptorBindings>,

    // Compute shader.
    present_set_index: Option<usize>,
    shader_module: Rc<ShaderModule>,
    shader_module_mtime: FileTime,
}

impl ShaderResources {
    pub unsafe fn new(
        device: &Rc<Device>,
        surface_info: &SurfaceInfo,
        present_name: &str,
        shader_path: &Path,
    ) -> Result<Self, Error> {
        // Compute shader.
        let shader_module_mtime = mtime(shader_path)?;
        let shader_module = ShaderModule::new(&device, shader_path)?;
        let present_set_index = shader_module
            .variable_declaration(present_name)
            .and_then(|declaration| declaration.set);

        // Descriptors.
        let descriptor_bindings = DescriptorBindings::new(&shader_module)?;
        let descriptor_layouts = DescriptorLayouts::new(&device, &descriptor_bindings)?;
        let descriptor_pool = DescriptorPool::new(
            &device,
            &descriptor_bindings,
            surface_info.desired_image_count,
        )?;
        let descriptor_sets = DescriptorSets::new(
            &device,
            &descriptor_layouts,
            &descriptor_pool,
            surface_info.desired_image_count,
        )?;

        // Pipelines.
        let pipeline_layout = PipelineLayout::new(&device, &shader_module, &descriptor_layouts)?;
        let pipeline = Pipeline::new(&device, &shader_module, &pipeline_layout)?;

        Ok(ShaderResources {
            shader_module_mtime,
            shader_module,
            present_set_index,
            descriptor_bindings,
            descriptor_layouts,
            descriptor_pool,
            descriptor_sets,
            pipeline_layout,
            pipeline,
        })
    }
}

// Define fields in reverse drop order.
pub struct Vulkan {
    // Other.
    pub num_frames: u32,
    pub binding_index: usize,

    image_acquired_semaphore: Rc<Semaphore>,
    compute_complete_semaphore: Rc<Semaphore>,
    reuse_command_buffer_fence: Rc<Fence>,

    // Shader modules, descriptor pools, sets and pipeline stuff.
    shader_resources: Vec<ShaderResources>,

    // Swapchain.
    present_name: String,
    swapchain_image_views: Vec<Rc<ImageView>>,
    swapchain_images: Vec<Rc<Image>>,
    swapchain: Option<Rc<Swapchain>>,
    swapchain_loader: Rc<SwapchainLoader>,

    // Staleness markers.
    image_binding_updates: Vec<ImageBindingUpdate>,
    buffer_binding_updates: Vec<BufferBindingUpdate>,
    stale_images: Vec<(Rc<Image>, vk::ImageLayout, vk::ImageLayout)>,
    stale_swapchain: bool,

    // Image data.
    sampler: Rc<Sampler>,
    image_subresource_range: ImageSubresourceRange,
    surface_info: SurfaceInfo,
    window_size: (u32, u32),

    // Device.
    command_buffer: Rc<CommandBuffer>,
    _command_pool: Rc<CommandPool>,
    compute_queue: Rc<ComputeQueue>,
    device: Rc<Device>,
    physical_device: Rc<PhysicalDevice>,
    surface: Rc<Surface>,
    surface_loader: Rc<SurfaceLoader>,

    // Core.
    _instance: Rc<Instance>,
    _entry: Rc<Entry>,
}

impl Vulkan {
    pub fn new(
        window: &Window,
        compute_shader_paths: &[impl Deref<Target = Path>],
    ) -> Result<Self, Error> {
        debug!("Initializing video system");
        unsafe {
            // Core.
            let entry = Entry::new()?;
            let instance = Instance::new(window, &entry)?;

            // Device.
            let surface_loader = SurfaceLoader::new(&entry, &instance)?;
            let surface = Surface::new(window, &entry, &instance, &surface_loader)?;
            let physical_device = PhysicalDevice::new(&instance, &surface)?;
            let device = Device::new(&instance, &physical_device)?;
            let compute_queue = ComputeQueue::new(&physical_device, &device)?;
            let command_pool = CommandPool::new(&physical_device, &device)?;
            let command_buffer = CommandBuffer::new(&device, &command_pool)?;

            // Image data.
            let window_size = (window.width, window.height);
            let surface_info =
                SurfaceInfo::new(window_size, &physical_device, &surface_loader, &surface)?;
            let image_subresource_range = ImageSubresourceRange::new()?;
            let sampler = Sampler::new(&device)?;

            // Staleness markers.
            let stale_swapchain = true;
            let stale_images = Vec::new();
            let buffer_binding_updates = Vec::new();
            let image_binding_updates = Vec::new();

            // Swapchain.
            let swapchain_loader = SwapchainLoader::new(&instance, &device)?;
            let swapchain = None;
            let swapchain_images = Vec::new();
            let swapchain_image_views = Vec::new();
            let present_name = "present".to_owned();

            let shader_resources = compute_shader_paths
                .iter()
                .map(|path| ShaderResources::new(&device, &surface_info, &present_name, path))
                .collect::<Result<_, Error>>()?;

            let reuse_command_buffer_fence = Fence::new(&device)?;
            let image_acquired_semaphore = Semaphore::new(&device)?;
            let compute_complete_semaphore = Semaphore::new(&device)?;

            Ok(Vulkan {
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
                image_subresource_range,
                sampler,
                stale_swapchain,
                stale_images,
                buffer_binding_updates,
                image_binding_updates,
                swapchain_loader,
                swapchain,
                swapchain_images,
                swapchain_image_views,
                present_name,
                shader_resources,
                reuse_command_buffer_fence,
                image_acquired_semaphore,
                compute_complete_semaphore,
                binding_index: 0,
                num_frames: 0,
            })
        }
    }

    fn swapchain(&self) -> &Rc<Swapchain> {
        match &self.swapchain {
            None => panic!("Did not expect missing swapchain here!"),
            Some(ref swapchain) => swapchain,
        }
    }

    unsafe fn recompile_shader_if_modified(&mut self) -> Result<(), Error> {
        for index in 0..self.shader_resources.len() {
            let path = &self.shader_resources[index].shader_module.source_path;
            if mtime(path)? > self.shader_resources[index].shader_module_mtime {
                info!("Recompiling {path:?}...");
                self.wait_idle();

                let new_resources = ShaderResources::new(
                    &self.device,
                    &self.surface_info,
                    &self.present_name,
                    &path,
                );

                match new_resources {
                    Ok(new_resources) => self.shader_resources[index] = new_resources,
                    Err(err) => error!("{err}"),
                }
            }
        }
        Ok(())
    }

    pub unsafe fn reinitialize_swapchain_if_needed(&mut self) -> Result<bool, Error> {
        if !self.stale_swapchain {
            return Ok(false);
        }
        self.stale_swapchain = false;

        debug!("Reinitializing swapchain");
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
        self.swapchain = Some(Swapchain::new(
            &self.surface,
            &self.surface_info,
            &self.swapchain_loader,
            self.swapchain.as_ref().map(|swapchain| ***swapchain),
        )?);
        self.swapchain_images =
            Image::many_from_swapchain(&self.swapchain_loader, self.swapchain())?;

        for image in self.swapchain_images.iter() {
            self.stale_images.push((
                image.clone(),
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::PRESENT_SRC_KHR,
            ));
        }
        self.swapchain_image_views = ImageView::many(
            &self.device,
            self.swapchain_images.iter(),
            &self.surface_info,
            &self.image_subresource_range,
        )?;
        let views_and_samplers = self
            .swapchain_image_views
            .iter()
            .map(|image_view| (image_view.clone(), self.sampler.clone()))
            .collect();
        self.image_binding_updates
            .push((self.present_name.clone(), views_and_samplers));

        Ok(true)
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

    unsafe fn transition_stale_images(&mut self) -> Result<(), Error> {
        self.reuse_command_buffer_fence.wait()?;
        self.reuse_command_buffer_fence.reset()?;
        self.begin_command_buffer()?;
        let stale_images = mem::take(&mut self.stale_images);
        for (image, old_layout, new_layout) in stale_images.into_iter() {
            self.image_memory_barrier_layout_transition(**image, old_layout, new_layout);
        }
        self.end_command_buffer()?;
        self.queue_submit_task()
    }

    unsafe fn acquire_next_image(&self) -> Result<(usize, vk::Image), Error> {
        let (present_index, _) = self.swapchain_loader.acquire_next_image(
            ***self.swapchain.as_ref().unwrap(),
            std::u64::MAX,
            **self.image_acquired_semaphore,
            vk::Fence::null(),
        )?;

        Ok((
            present_index as usize,
            **self.swapchain_images[present_index as usize],
        ))
    }

    unsafe fn push_constants(
        &self,
        pipeline_layout: &PipelineLayout,
        shader_module: &ShaderModule,
    ) {
        if let Some(declaration) = shader_module.push_constants_declaration() {
            // Prepare available fields.
            let mut avail_fields = HashMap::new();
            avail_fields.insert("frame_index".to_owned(), Value::U32(self.num_frames));

            // Allocate constants memory.
            let constants_size = declaration.byte_size().unwrap() as usize;
            let mut constants = vec![0; constants_size];
            let constants_ptr = constants.as_mut_ptr();

            // Write requested fields into memory.
            for field in &declaration.fields {
                let offset = field.offset.unwrap_or_else(|| {
                    warn!("Assuming offset 0, does this work correctly?");
                    0
                }) as usize;

                if let Some(..) = field.dimensions {
                    warn!("Don't know how to handle {field:?}");
                }

                match avail_fields.get(&field.name) {
                    None => error!("{} is not a registered push constant field", field.name),
                    Some(Value::U32(value)) => {
                        *constants_ptr.add(offset).cast() = value;
                    } // _ => warn!("Don't know how to handle {field:?}"),
                }
            }

            // Update on GPU.
            self.device.cmd_push_constants(
                **self.command_buffer,
                **pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                &constants,
            );
        }
    }

    unsafe fn bind_descriptor_set(
        &self,
        pipeline_layout: &PipelineLayout,
        present_set_index: Option<usize>,
        present_instance_index: usize,
        descriptor_sets: &DescriptorSets,
    ) {
        let descriptor_sets = descriptor_sets
            .iter()
            .enumerate()
            .map(|(set_index, descriptor_set)| {
                let instance_index = if Some(set_index) == present_set_index {
                    present_instance_index
                } else {
                    self.num_frames as usize % descriptor_set.len()
                };
                descriptor_set[instance_index]
            })
            .collect::<Vec<_>>();

        self.device.cmd_bind_descriptor_sets(
            **self.command_buffer,
            vk::PipelineBindPoint::COMPUTE,
            **pipeline_layout,
            0,
            &descriptor_sets,
            &[],
        );
    }

    unsafe fn dispatch(&self, shader_module: &ShaderModule) {
        let local_size = shader_module.local_size;
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
            .swapchains(&[***self.swapchain()])
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

        for resources in self.shader_resources.iter() {
            self.push_constants(&resources.pipeline_layout, &resources.shader_module);
            self.bind_descriptor_set(
                &resources.pipeline_layout,
                resources.present_set_index,
                present_index,
                &resources.descriptor_sets,
            );
            self.bind_pipeline(&resources.pipeline);
            self.dispatch(&resources.shader_module);
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

    pub unsafe fn tick(&mut self) -> Result<(), Error> {
        self.transition_stale_images()?;
        self.flush_binding_updates()?;
        self.recompile_shader_if_modified()?;
        if self.reinitialize_swapchain_if_needed()? {
            return Ok(());
        }
        self.render_next_frame()
    }
}

impl App for Vulkan {
    fn run_frame(&mut self) -> ControlFlow {
        match unsafe { self.tick() } {
            Err(error) => {
                error!("{error}");
                ControlFlow::ExitWithCode(1)
            }
            Ok(..) => ControlFlow::Poll,
        }
    }

    fn handle_resize(&mut self, new_size: (u32, u32)) -> Result<(), Error> {
        self.window_size = new_size;
        self.stale_swapchain = true;
        Ok(())
    }
}
