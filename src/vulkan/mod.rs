mod device;
mod fence;
pub mod instance;
mod physical_device;
mod pipeline;
mod semaphore;
mod shader_module;
mod surface;
mod surface_info;
mod swapchain;

use std::rc::Rc;

use self::{
    device::Device, fence::Fence, instance::Instance, physical_device::PhysicalDevice,
    semaphore::Semaphore, shader_module::ShaderModule, surface::Surface, swapchain::Swapchain,
};
use crate::{
    error::Error,
    vulkan::{pipeline::Pipeline, surface_info::SurfaceInfo},
    window::{App, Window},
};

use ash::vk;
use log::{debug, error, info, warn};
use winit::event_loop::ControlFlow;

struct PushConstants {
    num_frames: u32,
}

// Define fields in reverse drop order.
pub struct Vulkan {
    push_constants: PushConstants,

    window_size: (u32, u32),
    pub num_frames: u32,
    stale_image_layout: bool,
    stale_swapchain: bool,

    image_acquired_semaphore: Semaphore,
    compute_complete_semaphore: Semaphore,

    reuse_command_buffer_fence: Fence,

    pipeline: Pipeline<PushConstants>,
    compute_shader_module: ShaderModule,
    swapchain: Swapchain,
    surface_info: SurfaceInfo,
    device: Rc<Device>,
    physical_device: PhysicalDevice,
    surface: Surface,
    instance: Instance,
}

impl Vulkan {
    pub fn new(window: &Window) -> Result<Self, Error> {
        debug!("Initializing video system");

        let instance = Instance::new(window)?;
        let surface = Surface::new(&instance, window)?;
        let physical_device = PhysicalDevice::new(&instance, &surface)?;

        let device = Rc::new(Device::new(&instance, &physical_device)?);

        let window_size = (window.width, window.height);
        let surface_info = SurfaceInfo::new(window_size, &physical_device, &surface)?;
        let swapchain = Swapchain::new(&instance, device.clone(), &surface, &surface_info, None)?;

        let compute_shader_module = ShaderModule::new(device.clone())?;

        let pipeline = Pipeline::new(device.clone(), &compute_shader_module)?;
        swapchain.initialize_descriptor_sets(&pipeline.descriptor_sets);

        let reuse_command_buffer_fence = Fence::new(device.clone())?;

        let image_acquired_semaphore = Semaphore::new(device.clone())?;
        let compute_complete_semaphore = Semaphore::new(device.clone())?;

        let push_constants = PushConstants { num_frames: 0 };

        Ok(Vulkan {
            instance,
            surface,
            physical_device,
            device,
            surface_info,
            swapchain,
            compute_shader_module,
            pipeline,
            reuse_command_buffer_fence,
            image_acquired_semaphore,
            compute_complete_semaphore,
            window_size,
            num_frames: 0,
            // Images are in undefined layout when created.
            stale_image_layout: true,
            stale_swapchain: false,
            push_constants,
        })
    }

    pub fn recompile_shader_if_modified(&mut self) {
        if !self.compute_shader_module.was_modified() {
            return;
        }

        info!("Shader source modified, recompiling...");
        self.wait_idle();

        let compute_shader_module_result = self.compute_shader_module.rebuild();
        match compute_shader_module_result {
            Ok(compute_shader_module) => {
                let pipeline_result = Pipeline::new(self.device.clone(), &compute_shader_module);
                match pipeline_result {
                    Ok(pipeline) => {
                        self.swapchain
                            .initialize_descriptor_sets(&pipeline.descriptor_sets);
                        self.pipeline = pipeline;
                        self.compute_shader_module = compute_shader_module;
                    }
                    Err(pipeline_error) => {
                        error!("Failed to reinitialize pipeline: {:?}", pipeline_error)
                    }
                }
            }
            Err(shader_module_error) => {
                // Implement fmt::Display for this?
                warn!("Shader compilation failed: {:?}", shader_module_error)
            }
        }
    }

    pub fn reinitialize_after_resize(&mut self) -> Result<(), Error> {
        info!(
            "Reinitializing surface after resize to {:?}",
            self.window_size
        );

        self.wait_idle();

        let surface_info =
            SurfaceInfo::new(self.window_size, &self.physical_device, &self.surface)?;
        let swapchain = Swapchain::new(
            &self.instance,
            self.device.clone(),
            &self.surface,
            &surface_info,
            Some(self.swapchain.swapchain),
        )?;

        self.swapchain = swapchain;
        self.surface_info = surface_info;

        self.swapchain
            .initialize_descriptor_sets(&self.pipeline.descriptor_sets);

        Ok(())
    }

    pub fn transition_images_to_present(&self) {
        debug!("Transitioning image layout from undefined");

        self.reuse_command_buffer_fence.wait();
        self.reuse_command_buffer_fence.reset();

        self.device.begin_command_buffer();
        self.device.bind_pipeline(&self.pipeline);

        for image in self.swapchain.images.iter() {
            self.device.image_memory_barrier_layout_transition(
                *image,
                self.swapchain.image_subresource_range,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::PRESENT_SRC_KHR,
            );
        }

        self.device.end_command_buffer();
        self.device
            .queue_submit(None, None, &self.reuse_command_buffer_fence);
    }

    pub fn render_next_frame(&mut self) {
        self.push_constants.num_frames = self.num_frames;

        let (present_index, present_image) = self
            .swapchain
            .acquire_next_image(*self.image_acquired_semaphore);

        self.reuse_command_buffer_fence.wait();
        self.reuse_command_buffer_fence.reset();

        self.device.begin_command_buffer();
        self.device.bind_pipeline(&self.pipeline);

        // Transition image to "GENERAL" layout.
        self.device.image_memory_barrier_layout_transition(
            present_image,
            self.swapchain.image_subresource_range,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::ImageLayout::GENERAL,
        );

        self.pipeline.push_constants(&self.push_constants);
        self.pipeline.bind_descriptor_set(present_index);
        self.device.dispatch(100, 100, 1);

        // Transition image to the "PRESENT_SRC" layout for presentation.
        self.device.image_memory_barrier_layout_transition(
            present_image,
            self.swapchain.image_subresource_range,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );

        self.device.end_command_buffer();
        self.device.queue_submit(
            Some(&self.image_acquired_semaphore),
            Some(&self.compute_complete_semaphore),
            &self.reuse_command_buffer_fence,
        );

        // Present as soon as `compute_complete_semaphore` trips.
        let present_result = self.swapchain.present(
            self.device.compute_queue,
            present_index,
            &self.compute_complete_semaphore,
        );
        if let Err(Error::Vk(vk::Result::ERROR_OUT_OF_DATE_KHR)) = present_result {
            // Recreate swapchain and TODO
        }

        self.num_frames += 1;
    }

    pub fn wait_idle(&self) {
        unsafe { self.device.device_wait_idle() }.expect("Failed to wait for device idle")
    }
}

impl App for Vulkan {
    fn run_frame(&mut self) -> ControlFlow {
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
            self.transition_images_to_present();
            self.stale_image_layout = false;
        }

        self.recompile_shader_if_modified();
        self.render_next_frame();
        ControlFlow::Poll
    }

    fn handle_resize(&mut self, new_size: (u32, u32)) -> Result<(), Error> {
        self.window_size = new_size;
        self.stale_swapchain = true;
        Ok(())
    }
}
