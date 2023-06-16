use std::{collections::HashMap, mem, ops::Deref, path::Path, rc::Rc};

use ash::{
    extensions::khr::{PushDescriptor, Surface as SurfaceLoader, Swapchain as SwapchainLoader},
    vk,
};
use filetime::FileTime;
use log::{debug, error, info, warn};

use crate::{error::Error, utils::mtime, window::Window};

pub mod multi_buffer;
pub mod multi_image;
pub mod resources;

use self::resources::{
    buffer::Buffer, command_buffer::CommandBuffer, command_pool::CommandPool,
    descriptor_layout::DescriptorLayout, descriptors::Descriptors, device::Device, fence::Fence,
    image::Image, image_view::ImageView, instance::Instance, physical_device::PhysicalDevice,
    pipeline::Pipeline, pipeline_layout::PipelineLayout, sampler::Sampler, semaphore::Semaphore,
    shader_module::ShaderModule, surface::Surface, surface_info::SurfaceInfo, swapchain::Swapchain,
};

pub enum Event {
    Resized,
}

enum Value {
    U32(u32),
}

type AvailableImages = HashMap<
    String,
    Vec<(
        Rc<ImageView>,
        Rc<Sampler>,
        Box<[vk::DescriptorImageInfo; 1]>,
    )>,
>;
type AvailableBuffers = HashMap<String, Vec<(Rc<Buffer>, Box<[vk::DescriptorBufferInfo; 1]>)>>;

struct ShaderResources {
    // Pipelines.
    pipeline: Rc<Pipeline>,
    pipeline_layout: Rc<PipelineLayout>,

    // Descriptors.
    _descriptor_layout: Rc<DescriptorLayout>,
    descriptors: Descriptors,

    // Compute shader.
    shader_module: Rc<ShaderModule>,
    shader_module_mtime: FileTime,
}

impl ShaderResources {
    pub unsafe fn new(device: &Rc<Device>, shader_path: &Path) -> Result<Self, Error> {
        // Compute shader.
        let shader_module_mtime = mtime(shader_path)?;
        let shader_module = ShaderModule::new(device, shader_path)?;

        // Descriptors.
        let descriptors = Descriptors::new(&shader_module)?;
        let descriptor_layout = DescriptorLayout::new(device, &descriptors)?;

        // Pipelines.
        let pipeline_layout = PipelineLayout::new(device, &shader_module, &descriptor_layout)?;
        let pipeline = Pipeline::new(device, &shader_module, &pipeline_layout)?;

        Ok(ShaderResources {
            shader_module_mtime,
            shader_module,
            descriptors,
            _descriptor_layout: descriptor_layout,
            pipeline_layout,
            pipeline,
        })
    }

    fn invalidate_association_cache(&mut self) {
        if !self
            .descriptors
            .iter()
            .all(|descriptor| descriptor.instances.is_empty())
        {
            debug!(
                "Invalidating image and buffer binding cache for shader {:?}",
                self.shader_module.source_path
            );
            for descriptor in self.descriptors.iter_mut() {
                descriptor.instances.clear();
            }
        }
    }

    /// Collect, and, if missing, associate the available buffers with this shader.
    fn get_write_descriptor_set(
        &mut self,
        available_images: &AvailableImages,
        available_buffers: &AvailableBuffers,
        present_name: &str,
        present_index: usize,
        frame_index: usize,
    ) -> Result<Vec<vk::WriteDescriptorSet>, Error> {
        self.descriptors.get_write_descriptor_set(
            available_images,
            available_buffers,
            present_name,
            present_index,
            frame_index,
        )
    }
}

// Define fields in reverse drop order.
pub struct Vulkan {
    // Other.
    pub num_frames: usize,

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
    swapchain_loader: SwapchainLoader,

    // Resources.
    available_buffers: AvailableBuffers,
    available_images: AvailableImages,

    // Staleness markers.
    stale_images: Vec<(String, Rc<Image>, vk::ImageLayout, vk::ImageLayout)>,

    // Image data.
    sampler: Rc<Sampler>,
    image_subresource_range: vk::ImageSubresourceRange,
    pub surface_info: SurfaceInfo,
    vsync: bool,

    // Device.
    push_descriptor: PushDescriptor,
    command_buffer: Rc<CommandBuffer>,
    _command_pool: Rc<CommandPool>,
    compute_queue: vk::Queue,
    device: Rc<Device>,
    physical_device: Rc<PhysicalDevice>,
    surface: Rc<Surface>,
    surface_loader: SurfaceLoader,

    // Core.
    _instance: Rc<Instance>,
    _entry: ash::Entry,
}

impl Vulkan {
    pub fn new(
        window: &Window,
        compute_shader_paths: &[impl Deref<Target = Path>],
        vsync: bool,
    ) -> Result<Self, Error> {
        debug!("Initializing video system");
        unsafe {
            // Core.
            let entry = ash::Entry::linked();
            let instance = Instance::new(window, &entry)?;

            // Device.
            let surface_loader = SurfaceLoader::new(&entry, &instance);
            let surface = Surface::new(window, &entry, &instance, &surface_loader)?;
            let physical_device = PhysicalDevice::new(&instance, &surface)?;
            let device = Device::new(&instance, &physical_device)?;
            let compute_queue =
                device.get_device_queue(physical_device.compute_queue_family_index, 0);
            let command_pool = CommandPool::new(&physical_device, &device)?;
            let command_buffer = CommandBuffer::new(&device, &command_pool)?;
            let push_descriptor = PushDescriptor::new(&instance, &device);

            // Image data.
            let surface_info =
                SurfaceInfo::new(&physical_device, &surface_loader, &surface, vsync)?;
            let image_subresource_range = vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            };

            let sampler = Sampler::new(&device)?;

            // Staleness markers.
            let stale_images = Vec::new();

            // Resources.
            let available_images = HashMap::new();
            let available_buffers = HashMap::new();

            // Swapchain.
            let swapchain_loader = SwapchainLoader::new(&instance, &device);
            let swapchain = None;
            let swapchain_images = Vec::new();
            let swapchain_image_views = Vec::new();
            let present_name = "present".to_owned();

            let shader_resources = compute_shader_paths
                .iter()
                .map(|path| ShaderResources::new(&device, path))
                .collect::<Result<_, Error>>()?;

            let reuse_command_buffer_fence = Fence::new(&device)?;
            let image_acquired_semaphore = Semaphore::new(&device)?;
            let compute_complete_semaphore = Semaphore::new(&device)?;

            let mut vulkan = Vulkan {
                _entry: entry,
                _instance: instance,
                surface_loader,
                surface,
                physical_device,
                device,
                compute_queue,
                _command_pool: command_pool,
                command_buffer,
                push_descriptor,
                vsync,
                surface_info,
                image_subresource_range,
                sampler,
                stale_images,
                available_images,
                available_buffers,
                swapchain_loader,
                swapchain,
                swapchain_images,
                swapchain_image_views,
                present_name,
                shader_resources,
                reuse_command_buffer_fence,
                image_acquired_semaphore,
                compute_complete_semaphore,
                num_frames: 0,
            };

            vulkan.reinitialize_swapchain()?;

            Ok(vulkan)
        }
    }

    fn invalidate_shader_association_cache(&mut self) {
        for resources in &mut self.shader_resources {
            resources.invalidate_association_cache();
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
            let previous_mtime = self.shader_resources[index].shader_module_mtime;
            if mtime(path)? > previous_mtime {
                info!("Recompiling {path:?} ...");
                self.wait_idle();

                let new_resources = ShaderResources::new(&self.device, path);

                match new_resources {
                    Ok(new_resources) => self.shader_resources[index] = new_resources,
                    Err(err) => {
                        error!("{err}");
                        self.shader_resources[index].shader_module_mtime = mtime(path)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn register_image(&mut self, name: &str, views_and_samplers: &[(Rc<ImageView>, Rc<Sampler>)]) {
        let instances = views_and_samplers
            .iter()
            .map(|(image_view, sampler)| {
                (
                    image_view.clone(),
                    sampler.clone(),
                    Box::new([vk::DescriptorImageInfo::builder()
                        .image_view(***image_view)
                        .sampler(***sampler)
                        .image_layout(vk::ImageLayout::GENERAL)
                        .build()]),
                )
            })
            .collect();
        self.available_images.insert(name.to_owned(), instances);
        self.invalidate_shader_association_cache();
    }

    fn register_buffer(&mut self, name: &str, buffers: &[Rc<Buffer>]) {
        let instances = buffers
            .iter()
            .map(|buffer| {
                (
                    buffer.clone(),
                    Box::new([vk::DescriptorBufferInfo::builder()
                        .buffer(***buffer)
                        .offset(0)
                        .range(buffer.size)
                        .build()]),
                )
            })
            .collect();
        self.available_buffers.insert(name.to_owned(), instances);
        self.invalidate_shader_association_cache();
    }

    pub unsafe fn reinitialize_swapchain(&mut self) -> Result<(), Error> {
        debug!("Reinitializing swapchain");
        self.wait_idle();

        // After a resize (TODO Fix function name) all images need to be reinitialized anyway. The
        // case where not clearing this vector leads to issues is rather special, it occurs once on
        // app start and when there are two consecutive resize events.
        self.stale_images.clear();

        // TODO
        // The following code first creates the new resources, replaces them in `self` and only
        // then frees/drops the old ones. In case we are at memory limits this might leat to GPU
        // OOM errors. Alternative solutions: wrap all fields in `Option` or separate between free
        // and `drop`.

        self.surface_info = SurfaceInfo::new(
            &self.physical_device,
            &self.surface_loader,
            &self.surface,
            self.vsync,
        )?;
        self.swapchain = Some(Swapchain::new(
            &self.surface,
            &self.surface_info,
            &self.swapchain_loader,
            self.swapchain.as_ref().map(|swapchain| ***swapchain),
        )?);
        self.swapchain_images =
            Image::many_from_swapchain(&self.swapchain_loader, self.swapchain())?;

        for image in &self.swapchain_images {
            self.stale_images.push((
                self.present_name.clone(),
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
            .collect::<Vec<_>>();

        let present_name = self.present_name.clone();
        self.register_image(&present_name, &views_and_samplers);

        Ok(())
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
            self.compute_queue,
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
            .subresource_range(self.image_subresource_range)
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
        for (_, image, old_layout, new_layout) in stale_images {
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
            avail_fields.insert("frame_index".to_owned(), Value::U32(self.num_frames as u32));

            // Allocate constants memory.
            let constants_size = declaration.byte_size().unwrap() as usize;
            let mut constants = vec![0u8; constants_size];
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
                        *constants_ptr.add(offset).cast::<u32>() = *value;
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

    unsafe fn push_descriptors(
        &self,
        pipeline_layout: &PipelineLayout,
        write_descriptor_set: &[vk::WriteDescriptorSet],
    ) {
        self.push_descriptor.cmd_push_descriptor_set(
            **self.command_buffer,
            vk::PipelineBindPoint::COMPUTE,
            **pipeline_layout,
            0,
            write_descriptor_set,
        );
    }

    unsafe fn dispatch(&self, shader_module: &ShaderModule) {
        let local_size = shader_module.local_size;
        let window_size = self.surface_info.surface_resolution;
        let invocation_x = window_size.width / local_size.0;
        let invocation_y = window_size.height / local_size.1;
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
            .queue_present(self.compute_queue, &present_info)
            .map(|suboptimal| {
                if suboptimal {
                    warn!("Swapchain is suboptimal");
                }
            })?)
    }

    unsafe fn render_next_frame(&mut self) -> Result<Option<Event>, Error> {
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

        for index in 0..self.shader_resources.len() {
            let write_descriptor_set = self.shader_resources[index].get_write_descriptor_set(
                &self.available_images,
                &self.available_buffers,
                &self.present_name,
                present_index,
                self.num_frames,
            )?;
            let resources = &self.shader_resources[index];

            self.bind_pipeline(&resources.pipeline);
            self.push_constants(&resources.pipeline_layout, &resources.shader_module);
            self.push_descriptors(&resources.pipeline_layout, &write_descriptor_set);
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
        self.num_frames += 1;

        if let Err(Error::Vk(vk::Result::ERROR_OUT_OF_DATE_KHR)) = present_result {
            self.reinitialize_swapchain()?;
            Ok(Some(Event::Resized))
        } else {
            Ok(None)
        }
    }

    pub fn wait_idle(&self) {
        unsafe {
            self.device
                .device_wait_idle()
                .expect("Failed to wait for device idle");
        }
    }

    pub unsafe fn tick(&mut self) -> Result<Option<Event>, Error> {
        self.transition_stale_images()?;
        self.recompile_shader_if_modified()?;
        self.render_next_frame()
    }
}
