extern crate ash;
extern crate ash_window;
extern crate libpulse_binding as pulse;
extern crate raw_window_handle;
extern crate winit;

mod high_device;
mod high_swapchain;

use log::{debug, error, log_enabled, info, Level};
use ash::{
    extensions::khr::{Surface as SurfaceLoader, Swapchain as SwapchainLoader},
    prelude::VkResult,
    vk::{
        self, CommandBuffer, CommandPool, DependencyFlags, DescriptorImageInfo, DescriptorPool,
        DescriptorPoolCreateInfo, DescriptorPoolSize, DescriptorSetAllocateInfo,
        DescriptorSetLayout, DescriptorSetLayoutBinding, DescriptorSetLayoutCreateInfo,
        DescriptorType, Extent2D, ImageLayout, ImageMemoryBarrier, ImageView, Pipeline,
        PipelineBindPoint, PipelineLayout, PipelineStageFlags, PresentModeKHR, ShaderModule,
        ShaderStageFlags, SurfaceCapabilitiesKHR, SurfaceFormatKHR, SurfaceKHR as Surface,
        SwapchainKHR as Swapchain, WriteDescriptorSet,
    },
    Device, Entry, Instance,
};
use core::ops::Not;
use pulse::{
    context::{Context, FlagSet as ContextFlagSet},
    def::Retval,
    mainloop::standard::{IterateResult, Mainloop},
    proplist::Proplist,
    sample::{Format, Spec},
    stream::{FlagSet as StreamFlagSet, Stream},
};
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use std::{
    cell::RefCell,
    ffi::CStr,
    fs,
    io::{self, Cursor},
    ops::Deref,
    process::Command,
    rc::Rc,
    slice,
};
use winit::{
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    platform::run_return::EventLoopExtRunReturn,
    window::{Window as WinitWindow, WindowBuilder},
};

use crate::{high_device::HighDevice, high_swapchain::HighSwapchain};

struct Window {
    window_size: (u32, u32),
    event_loop: EventLoop<()>,
    window: WinitWindow,
}

fn init_window(window_size: (u32, u32)) -> Window {
    let event_loop = EventLoop::new();
    let size = winit::dpi::LogicalSize::new(1280, 1024);
    let window = WindowBuilder::new()
        .with_title("visualize-rs")
        .with_inner_size(size)
        .build(&event_loop)
        .expect("Failed to create window");

    Window {
        window_size,
        event_loop,
        window,
    }
}

fn create_instance(window: &Window, entry: &Entry) -> VkResult<Instance> {
    let app_info = vk::ApplicationInfo::builder().api_version(vk::make_api_version(0, 1, 3, 0));
    let raw_handle = window.window.raw_display_handle();
    let extension_names = ash_window::enumerate_required_extensions(raw_handle)?.to_vec();

    // List available layers. TODO check that the validation layer exists.
    let layer_properties = entry.enumerate_instance_layer_properties()?;
    let validation_layer =
        unsafe { CStr::from_bytes_with_nul_unchecked(b"VK_LAYER_KHRONOS_validation\0") };
    let layer_names = [validation_layer.as_ptr()];

    let create_info = vk::InstanceCreateInfo::builder()
        .application_info(&app_info)
        .enabled_extension_names(&extension_names)
        .enabled_layer_names(&layer_names);

    let instance = unsafe { entry.create_instance(&create_info, None) }?;

    Ok(instance)
}

fn choose_compute_queue_family(
    _physical_device: &vk::PhysicalDevice,
    (index, queue_family_properties): (usize, &vk::QueueFamilyProperties),
) -> Option<u32> {
    let queue_flags = queue_family_properties.queue_flags;
    let supports_compute = queue_flags.contains(vk::QueueFlags::COMPUTE);
    let does_not_support_graphics = queue_flags.not().contains(vk::QueueFlags::GRAPHICS);

    if supports_compute && does_not_support_graphics {
        Some(index as u32)
    } else {
        None
    }
}

fn choose_render_queue_family(
    surface_loader: &SurfaceLoader,
    surface: &Surface,
    physical_device: &vk::PhysicalDevice,
    (index, queue_family_properties): (usize, &vk::QueueFamilyProperties),
) -> Option<u32> {
    let supports_graphics = queue_family_properties
        .queue_flags
        .contains(vk::QueueFlags::GRAPHICS);
    let supports_surface = unsafe {
        surface_loader.get_physical_device_surface_support(*physical_device, index as u32, *surface)
    }
    .expect("Failed to get physical device surface support info");

    if supports_graphics && supports_surface {
        Some(index as u32)
    } else {
        None
    }
}

/// Search for a compute queue and a render queue in a physical device.
fn choose_physical_device_queues(
    instance: &Instance,
    surface_loader: &SurfaceLoader,
    surface: &Surface,
    physical_device: &vk::PhysicalDevice,
) -> Option<(vk::PhysicalDevice, u32, u32)> {
    let queue_family_properties =
        unsafe { instance.get_physical_device_queue_family_properties(*physical_device) };

    let compute_queue_family_index = queue_family_properties
        .iter()
        .enumerate()
        .find_map(|queue_family| choose_compute_queue_family(physical_device, queue_family))?;
    let render_queue_family_index =
        queue_family_properties
            .iter()
            .enumerate()
            .find_map(|queue_family| {
                choose_render_queue_family(surface_loader, surface, physical_device, queue_family)
            })?;

    Some((
        *physical_device,
        compute_queue_family_index,
        render_queue_family_index,
    ))
}

/// Select a physical device that has a compute queue and a render queue.
fn choose_physical_device(
    instance: &Instance,
    surface_loader: &SurfaceLoader,
    surface: &Surface,
) -> (vk::PhysicalDevice, u32, u32) {
    let physical_devices: Vec<vk::PhysicalDevice> =
        unsafe { instance.enumerate_physical_devices() }
            .expect("Failed to enumerate physical devices");
    physical_devices
        .iter()
        .find_map(|physical_device| {
            choose_physical_device_queues(instance, surface_loader, surface, physical_device)
        })
        .expect("Couldn't find suitable device.")
}

fn create_device(
    instance: &Instance,
    physical_device: &vk::PhysicalDevice,
    compute_queue_family_index: u32,
    render_queue_family_index: u32,
) -> Device {
    let create_infos = {
        let compute_queue_create_info = vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(compute_queue_family_index)
            .queue_priorities(&[1.0])
            .build();
        let render_queue_create_info = vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(render_queue_family_index)
            .queue_priorities(&[1.0])
            .build();

        &[compute_queue_create_info, render_queue_create_info]
    };
    let device_extension_names_raw = [SwapchainLoader::name().as_ptr()];

    let features = vk::PhysicalDeviceFeatures::default();

    let device_create_info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(create_infos)
        .enabled_extension_names(&device_extension_names_raw)
        .enabled_features(&features);

    unsafe { instance.create_device(*physical_device, &device_create_info, None) }
        .expect("Failed to create device")
}

struct SurfaceInfo {
    pub surface_format: SurfaceFormatKHR,
    pub surface_capabilities: SurfaceCapabilitiesKHR,
    pub desired_present_mode: PresentModeKHR,
    pub desired_image_count: u32,
    pub surface_resolution: Extent2D,
}

fn get_physical_device_surface_info(
    window: &Window,
    physical_device: &vk::PhysicalDevice,
    surface_loader: &SurfaceLoader,
    surface: &Surface,
) -> VkResult<SurfaceInfo> {
    let (surface_format, surface_capabilities, present_modes) = unsafe {
        (
            surface_loader.get_physical_device_surface_formats(*physical_device, *surface)?[0],
            surface_loader.get_physical_device_surface_capabilities(*physical_device, *surface)?,
            surface_loader.get_physical_device_surface_present_modes(*physical_device, *surface)?,
        )
    };

    // For reference see:
    // https://www.reddit.com/r/vulkan/comments/9txqqb/what_is_presentation_mode/
    let &desired_present_mode = present_modes
        .iter()
        .find(|&&mode| mode == vk::PresentModeKHR::FIFO)
        .expect("There is no vsync present mode");

    // Check that the surface supports storage write/can be used in compute shaders.
    if !surface_capabilities
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::STORAGE)
    {
        return Err(vk::Result::ERROR_FEATURE_NOT_PRESENT);
    }

    // Try to get triple buffering, fall back to double-buffering.
    // Assuming all modern GPUs support double buffering.
    let min_image_count = surface_capabilities.min_image_count;
    let max_image_count = surface_capabilities.max_image_count;
    let mut desired_image_count = min_image_count + 1;
    if max_image_count != 0 && desired_image_count > max_image_count {
        desired_image_count = max_image_count;
    }

    let (width, height) = window.window_size;
    let surface_resolution = match surface_capabilities.current_extent.width {
        std::u32::MAX => vk::Extent2D { width, height },
        _ => surface_capabilities.current_extent,
    };

    Ok(SurfaceInfo {
        surface_format,
        surface_capabilities,
        desired_present_mode,
        desired_image_count,
        surface_resolution,
    })
}

fn create_swapchain(
    instance: &Instance,
    device: &Device,
    surface: &Surface,
    surface_info: &SurfaceInfo,
) -> VkResult<(SwapchainLoader, Swapchain)> {
    let surface_format = &surface_info.surface_format;

    let swapchain_loader = SwapchainLoader::new(&instance, &device);
    let swapchain_create_info = vk::SwapchainCreateInfoKHR::builder()
        .surface(*surface)
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
        .image_array_layers(1);

    let swapchain = unsafe { swapchain_loader.create_swapchain(&swapchain_create_info, None) }?;

    Ok((swapchain_loader, swapchain))
}

fn create_command_pool_and_buffer(
    device: &Device,
    queue_family_index: u32,
) -> VkResult<(CommandPool, CommandBuffer)> {
    let pool_create_info = vk::CommandPoolCreateInfo::builder()
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
        .queue_family_index(queue_family_index);

    let pool = unsafe { device.create_command_pool(&pool_create_info, None) }?;

    let buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
        .command_buffer_count(1)
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY);

    let command_buffer = unsafe { device.allocate_command_buffers(&buffer_allocate_info) }?[0];

    Ok((pool, command_buffer))
}

/// Decode SPIR-V from bytes.
///
/// This function handles SPIR-V of arbitrary endianness gracefully, and returns correctly aligned
/// storage.
///
/// # Examples
/// ```no_run
/// // Decode SPIR-V from a file
/// let mut file = std::fs::File::open("/path/to/shader.spv").unwrap();
/// let words = ash::util::read_spv(&mut file).unwrap();
/// ```
/// ```
/// // Decode SPIR-V from memory
/// const SPIRV: &[u8] = &[
///     // ...
/// #   0x03, 0x02, 0x23, 0x07,
/// ];
/// let words = ash::util::read_spv(&mut std::io::Cursor::new(&SPIRV[..])).unwrap();
/// ```
pub fn read_spv<R: io::Read + io::Seek>(x: &mut R) -> io::Result<Vec<u32>> {
    let size = x.seek(io::SeekFrom::End(0))?;
    if size % 4 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "input length not divisible by 4",
        ));
    }
    if size > usize::max_value() as u64 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "input too long"));
    }
    let words = (size / 4) as usize;
    // https://github.com/MaikKlein/ash/issues/354:
    // Zero-initialize the result to prevent read_exact from possibly
    // reading uninitialized memory.
    let mut result = vec![0u32; words];
    x.seek(io::SeekFrom::Start(0))?;
    x.read_exact(unsafe { slice::from_raw_parts_mut(result.as_mut_ptr() as *mut u8, words * 4) })?;
    const MAGIC_NUMBER: u32 = 0x0723_0203;
    if !result.is_empty() && result[0] == MAGIC_NUMBER.swap_bytes() {
        for word in &mut result {
            *word = word.swap_bytes();
        }
    }
    if result.is_empty() || result[0] != MAGIC_NUMBER {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "input missing SPIR-V magic number",
        ));
    }
    Ok(result)
}

fn create_shader_module(device: &Device) -> VkResult<ShaderModule> {
    println!("Compiling shader");
    Command::new("glslc")
        .args(["shaders/debug.comp", "-o", "shaders/debug.spv"])
        .output()
        .unwrap();

    println!("Initializing shader module");
    let mut compute_shader_spirv_bytes = Cursor::new(fs::read("shaders/debug.spv").unwrap());
    let compute_shader_content = read_spv(&mut compute_shader_spirv_bytes).unwrap();

    let compute_shader_info = vk::ShaderModuleCreateInfo::builder().code(&compute_shader_content);
    let compute_shader_module = unsafe { device.create_shader_module(&compute_shader_info, None) }?;

    Ok(compute_shader_module)
}

fn create_descriptor_set_layout(device: &Device) -> VkResult<DescriptorSetLayout> {
    let descriptor_set_layout_binding = DescriptorSetLayoutBinding::builder()
        .binding(0)
        .descriptor_type(DescriptorType::STORAGE_IMAGE)
        .descriptor_count(1)
        .stage_flags(ShaderStageFlags::COMPUTE);
    // TODO immutable samplers?
    let descriptor_set_layout_bindings = [*descriptor_set_layout_binding];
    let descriptor_set_layout_create_info =
        DescriptorSetLayoutCreateInfo::builder().bindings(&descriptor_set_layout_bindings);
    let descriptor_set_layout =
        unsafe { device.create_descriptor_set_layout(&descriptor_set_layout_create_info, None) }?;
    Ok(descriptor_set_layout)
}

fn create_compute_pipeline_layout(
    device: &Device,
    descriptor_set_layout: &DescriptorSetLayout,
) -> VkResult<PipelineLayout> {
    let descriptor_set_layouts = [*descriptor_set_layout];

    // Layout.
    let layout_create_info =
        vk::PipelineLayoutCreateInfo::builder().set_layouts(&descriptor_set_layouts);
    let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_create_info, None) }?;

    Ok(pipeline_layout)
}

fn create_compute_pipeline(
    device: &Device,
    pipeline_layout: &PipelineLayout,
    shader_module: &ShaderModule,
) -> VkResult<Pipeline> {
    // Shader stage.
    let shader_entry_name = unsafe { CStr::from_bytes_with_nul_unchecked(b"main\0") };
    let shader_stage_create_info = vk::PipelineShaderStageCreateInfo {
        module: *shader_module,
        p_name: shader_entry_name.as_ptr(),
        stage: vk::ShaderStageFlags::COMPUTE,
        ..Default::default()
    };

    // Pipeline.
    let compute_pipeline_create_info = vk::ComputePipelineCreateInfo::builder()
        .stage(shader_stage_create_info)
        .layout(*pipeline_layout)
        .build();
    let pipelines = unsafe {
        device.create_compute_pipelines(
            vk::PipelineCache::null(),
            &[compute_pipeline_create_info],
            None,
        )
    }
    .unwrap();

    let pipeline = pipelines[0];
    Ok(pipeline)
}

fn create_descriptor_pool(device: &Device) -> VkResult<DescriptorPool> {
    let descriptor_pool_size = DescriptorPoolSize::builder()
        .ty(DescriptorType::STORAGE_IMAGE)
        .descriptor_count(3); // TODO

    let descriptor_pool_sizes = [*descriptor_pool_size];
    let pool_create_info = DescriptorPoolCreateInfo::builder()
        .pool_sizes(&descriptor_pool_sizes)
        .max_sets(3); // TODO
    let descriptor_pool = unsafe { device.create_descriptor_pool(&*pool_create_info, None) }?;

    Ok(descriptor_pool)
}

fn init_vulkan(window: &mut Window) -> VkResult<()> {
    println!("Initializing Vulkan");
    let entry = Entry::linked();
    let instance = create_instance(&window, &entry)?;

    println!("Initializing surface from window");
    let surface_loader = SurfaceLoader::new(&entry, &instance);
    let surface = unsafe {
        ash_window::create_surface(
            &entry,
            &instance,
            window.window.raw_display_handle(),
            window.window.raw_window_handle(),
            None,
        )
    }?;

    println!("Initializing device");
    let (physical_device, compute_queue_family_index, render_queue_family_index) =
        choose_physical_device(&instance, &surface_loader, &surface);
    let device = create_device(
        &instance,
        &physical_device,
        compute_queue_family_index,
        render_queue_family_index,
    );

    println!("Initializing swapchain");
    let surface_info =
        get_physical_device_surface_info(window, &physical_device, &surface_loader, &surface)?;
    let (swapchain_loader, swapchain) =
        create_swapchain(&instance, &device, &surface, &surface_info)?;

    println!("Initializing queues");
    let compute_queue = unsafe { device.get_device_queue(compute_queue_family_index as u32, 0) };
    let _render_queue = unsafe { device.get_device_queue(render_queue_family_index as u32, 0) };

    println!("Compiling shader");
    let compute_shader_module = create_shader_module(&device)?;

    println!("Creating descriptor set layout");
    let descriptor_set_layout = create_descriptor_set_layout(&device)?;

    println!("Initializing compute pipeline");
    let pipeline_layout = create_compute_pipeline_layout(&device, &descriptor_set_layout)?;
    let compute_pipeline =
        create_compute_pipeline(&device, &pipeline_layout, &compute_shader_module)?;

    println!("Initializing command buffers");
    let (_compute_command_pool, compute_command_buffer) =
        create_command_pool_and_buffer(&device, compute_queue_family_index)?;
    // Do i need the renderqueue?
    let (_render_command_pool, _render_command_buffer) =
        create_command_pool_and_buffer(&device, render_queue_family_index)?;

    println!("Initializing descriptor sets");
    let descriptor_pool = create_descriptor_pool(&device)?;
    let descriptor_set_layouts = vec![descriptor_set_layout; 3];
    let descriptor_set_allocate_info = DescriptorSetAllocateInfo::builder()
        .descriptor_pool(descriptor_pool)
        .set_layouts(descriptor_set_layouts.as_slice());
    let descriptor_sets =
        unsafe { device.allocate_descriptor_sets(&descriptor_set_allocate_info) }?;

    println!("Building views for swapchain images");
    let image_subresource_range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    };

    let swapchain_images = unsafe { swapchain_loader.get_swapchain_images(swapchain) }?;
    let swapchain_image_views: Vec<ImageView> = swapchain_images
        .iter()
        .map(|&image| {
            let component_mapping = vk::ComponentMapping {
                r: vk::ComponentSwizzle::R,
                g: vk::ComponentSwizzle::G,
                b: vk::ComponentSwizzle::B,
                a: vk::ComponentSwizzle::A,
            };
            let create_view_info = vk::ImageViewCreateInfo::builder()
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(surface_info.surface_format.format)
                .components(component_mapping)
                .subresource_range(image_subresource_range)
                .image(image);
            unsafe { device.create_image_view(&create_view_info, None) }.unwrap()
        })
        .collect();

    println!("Creating sampler");
    let sampler_info = vk::SamplerCreateInfo {
        mag_filter: vk::Filter::LINEAR,
        min_filter: vk::Filter::LINEAR,
        mipmap_mode: vk::SamplerMipmapMode::LINEAR,
        address_mode_u: vk::SamplerAddressMode::MIRRORED_REPEAT,
        address_mode_v: vk::SamplerAddressMode::MIRRORED_REPEAT,
        address_mode_w: vk::SamplerAddressMode::MIRRORED_REPEAT,
        max_anisotropy: 1.0,
        border_color: vk::BorderColor::FLOAT_OPAQUE_WHITE,
        compare_op: vk::CompareOp::NEVER,
        ..Default::default()
    };
    let sampler = unsafe { device.create_sampler(&sampler_info, None) }?;

    println!("Writing descriptor sets");
    // Image views need to exist until `write_descriptor_sets has been called. Note the difference
    // between passing a reference and a reference to a desctructured reference.
    let image_infos_vec: Vec<[DescriptorImageInfo; 1]> = swapchain_image_views
        .iter()
        .map(|&image_view| {
            let image_info = DescriptorImageInfo::builder()
                .image_view(image_view)
                .sampler(sampler)
                .image_layout(ImageLayout::GENERAL)
                .build();
            [image_info]
        })
        .collect();

    let write_descriptor_sets: Vec<WriteDescriptorSet> = image_infos_vec
        .iter()
        .zip(descriptor_sets.iter())
        .map(|(image_infos, &descriptor_set)| {
            let a = WriteDescriptorSet::builder()
                .descriptor_type(DescriptorType::STORAGE_IMAGE)
                .image_info(image_infos)
                .dst_set(descriptor_set)
                .dst_binding(0)
                .dst_array_element(0)
                .build();

            return a;
        })
        .collect();

    unsafe { device.update_descriptor_sets(&write_descriptor_sets, &[]) };

    let mut high_device = HighDevice::new(&device);
    let high_swapchain = HighSwapchain::new(&swapchain_loader, &swapchain);

    let image_acquired_semaphore = high_device.create_semaphore();
    let compute_complete_semaphore = high_device.create_semaphore();

    let mut num_frames: u32 = 0;

    let mut render_next_frame = |num_frames: &mut u32| {
        let present_index = high_swapchain.acquire_next_image(image_acquired_semaphore);
        let present_image = swapchain_images[present_index as usize];
        high_device.begin_command_buffer(compute_command_buffer);
        high_device.bind_pipeline(PipelineBindPoint::COMPUTE, compute_pipeline);

        let source_layout = if *num_frames > surface_info.desired_image_count {
            ImageLayout::PRESENT_SRC_KHR
        } else {
            ImageLayout::UNDEFINED
        };

        high_device.image_memory_barrier_layout_transition(
            present_image,
            image_subresource_range,
            source_layout,
            ImageLayout::GENERAL,
        );
        let bind_descriptor_set = descriptor_sets[present_index as usize];
        high_device.bind_descriptor_set(pipeline_layout, bind_descriptor_set);
        high_device.dispatch(100, 100, 1);
        high_device.image_memory_barrier_layout_transition(
            present_image,
            image_subresource_range,
            ImageLayout::GENERAL,
            ImageLayout::PRESENT_SRC_KHR,
        );
        high_device.end_command_buffer();
        high_device.queue_submit(compute_queue, compute_complete_semaphore);
        high_swapchain.present(compute_queue, present_index, compute_complete_semaphore);

        *num_frames += 1;
    };

    window
        .event_loop
        .run_return(|event: Event<()>, &_, control_flow: &mut ControlFlow| {
            *control_flow = ControlFlow::Poll;
            match event {
                Event::WindowEvent {
                    event:
                        WindowEvent::CloseRequested
                        | WindowEvent::KeyboardInput {
                            input:
                                KeyboardInput {
                                    state: ElementState::Pressed,
                                    virtual_keycode: Some(VirtualKeyCode::Escape),
                                    ..
                                },
                            ..
                        },
                    ..
                } => *control_flow = ControlFlow::Exit,
                Event::MainEventsCleared => {
                    println!("\n\nNext frame\n");
                    render_next_frame(&mut num_frames);
                    if num_frames == 5 {
                        *control_flow = ControlFlow::Exit;
                    }
                }
                _ => (),
            }
        });

    Ok(())
}

fn init_pulse() {
    let spec = Spec {
        format: Format::S16NE,
        channels: 2,
        rate: 44100,
    };
    assert!(spec.is_valid());

    let mut proplist = Proplist::new().unwrap();
    proplist
        .set_str(pulse::proplist::properties::APPLICATION_NAME, "FooApp")
        .unwrap();

    let mainloop = Rc::new(RefCell::new(
        Mainloop::new().expect("Failed to create mainloop"),
    ));

    let context = Rc::new(RefCell::new(
        Context::new_with_proplist(mainloop.borrow().deref(), "FooAppContext", &proplist)
            .expect("Failed to create new context"),
    ));

    context
        .borrow_mut()
        .connect(None, ContextFlagSet::NOFLAGS, None)
        .expect("Failed to connect context");

    // Wait for context to be ready
    loop {
        match mainloop.borrow_mut().iterate(false) {
            IterateResult::Quit(_) | IterateResult::Err(_) => {
                eprintln!("Iterate state was not success, quitting...");
                return;
            }
            IterateResult::Success(_) => {}
        }
        match context.borrow().get_state() {
            pulse::context::State::Ready => {
                break;
            }
            pulse::context::State::Failed | pulse::context::State::Terminated => {
                eprintln!("Context state failed/terminated, quitting...");
                return;
            }
            _ => {}
        }
    }

    let stream = Rc::new(RefCell::new(
        Stream::new(&mut context.borrow_mut(), "Music", &spec, None)
            .expect("Failed to create new stream"),
    ));

    stream
        .borrow_mut()
        .connect_playback(None, None, StreamFlagSet::START_CORKED, None, None)
        .expect("Failed to connect playback");

    // Wait for stream to be ready
    loop {
        match mainloop.borrow_mut().iterate(false) {
            IterateResult::Quit(_) | IterateResult::Err(_) => {
                eprintln!("Iterate state was not success, quitting...");
                return;
            }
            IterateResult::Success(_) => {}
        }
        match stream.borrow().get_state() {
            pulse::stream::State::Ready => {
                break;
            }
            pulse::stream::State::Failed | pulse::stream::State::Terminated => {
                eprintln!("Stream state failed/terminated, quitting...");
                return;
            }
            _ => {}
        }
    }

    // Our main logic (to output a stream of audio data)
    let _drained = Rc::new(RefCell::new(false));
    // loop {
    //     match mainloop.borrow_mut().iterate(false) {
    //         IterateResult::Quit(_) | IterateResult::Err(_) => {
    //             eprintln!("Iterate state was not success, quitting...");
    //             return;
    //         }
    //         IterateResult::Success(_) => {}
    //     }
    //
    //     // Write some data with stream.write()
    //
    //     if stream.borrow().is_corked().unwrap() {
    //         stream.borrow_mut().uncork(None);
    //     }
    //
    //     // Wait for our data to be played
    //     let _o = {
    //         let drain_state_ref = Rc::clone(&drained);
    //         stream
    //             .borrow_mut()
    //             .drain(Some(Box::new(move |_success: bool| {
    //                 *drain_state_ref.borrow_mut() = true;
    //             })))
    //     };
    //     while *drained.borrow_mut() == false {
    //         match mainloop.borrow_mut().iterate(false) {
    //             IterateResult::Quit(_) | IterateResult::Err(_) => {
    //                 eprintln!("Iterate state was not success, quitting...");
    //                 return;
    //             }
    //             IterateResult::Success(_) => {}
    //         }
    //     }
    //     *drained.borrow_mut() = false;
    //
    //     // Remember to break out of the loop once done writing all data (or whatever).
    // }

    // Clean shutdown
    mainloop.borrow_mut().quit(Retval(0)); // uncertain whether this is necessary
    stream.borrow_mut().disconnect().unwrap();
}

fn main() {
    env_logger::init();


    let window_size = (1280, 1024);

    let mut window = init_window(window_size);
    let _vulkan = init_vulkan(&mut window);
    // let _pulse = init_pulse();
}
