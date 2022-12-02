extern crate ash;
extern crate ash_window;
extern crate libpulse_binding as pulse;
extern crate raw_window_handle;
extern crate winit;

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
    window::Window,
};

use ash::vk;
use log::{debug, error, info, warn};

// Define fields in reverse drop order.
pub struct Vulkan {
    pub num_frames: u32,

    image_acquired_semaphore: Semaphore,
    compute_complete_semaphore: Semaphore,

    reuse_command_buffer_fence: Fence,

    pipeline: Pipeline,
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

        let instance = Instance::new(&window)?;
        let surface = Surface::new(&instance, window)?;
        let physical_device = PhysicalDevice::new(&instance, &surface)?;

        let device = Rc::new(Device::new(&instance, &physical_device)?);

        let surface_info = SurfaceInfo::new(window, &physical_device, &surface)?;
        let swapchain = Swapchain::new(&instance, device.clone(), &surface, &surface_info)?;

        let compute_shader_module = ShaderModule::new(device.clone())?;

        let pipeline = Pipeline::new(device.clone(), &compute_shader_module)?;
        swapchain.initialize_descriptor_sets(&pipeline.descriptor_sets);

        let reuse_command_buffer_fence = Fence::new(device.clone())?;

        let image_acquired_semaphore = Semaphore::new(device.clone())?;
        let compute_complete_semaphore = Semaphore::new(device.clone())?;

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
            num_frames: 0,
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

    fn reinitialize_after_resize(&mut self, window: &Window) {
        info!("Reinitializing surface after resize");

        let surface = Surface::new(&self.instance, window)?;
        let physical_device = PhysicalDevice::new(&self.instance, &surface)?;

        let device = Rc::new(Device::new(&instance, &physical_device)?);

        let surface_info = SurfaceInfo::new(window, &physical_device, &surface)?;
        let swapchain = Swapchain::new(&instance, device.clone(), &surface, &surface_info)?;

        swapchain.initialize_descriptor_sets(&pipeline.descriptor_sets);
    }

    pub fn render_next_frame(&mut self) {
        let (present_index, present_image) = self
            .swapchain
            .acquire_next_image(*self.image_acquired_semaphore);

        self.reuse_command_buffer_fence.wait();
        self.reuse_command_buffer_fence.reset();

        self.device.begin_command_buffer();
        self.device.bind_pipeline(&self.pipeline);

        // Transition image to "GENERAL" layout.
        let source_layout = if self.num_frames > self.surface_info.desired_image_count {
            vk::ImageLayout::PRESENT_SRC_KHR
        } else {
            vk::ImageLayout::UNDEFINED
        };

        self.device.image_memory_barrier_layout_transition(
            present_image,
            self.swapchain.image_subresource_range,
            source_layout,
            vk::ImageLayout::GENERAL,
        );

        self.pipeline.push_constants(self.num_frames);
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
            &self.image_acquired_semaphore,
            &self.compute_complete_semaphore,
            &self.reuse_command_buffer_fence,
        );

        // Present as soon as `compute_complete_semaphore` trips.
        let present_result = self.swapchain.present(
            self.device.compute_queue,
            present_index,
            &self.compute_complete_semaphore,
        );
        match present_result {
            Err(Error::VkError(vk::Result::ERROR_OUT_OF_DATE_KHR)) => {
                // Recreate swapchain and
            }
            _ => {}
        }

        self.num_frames += 1;
    }

    pub fn wait_idle(&self) {
        unsafe { self.device.device_wait_idle() }.expect("Failed to wait for device idle")
    }
}
