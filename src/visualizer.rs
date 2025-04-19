use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    rc::Rc,
    time::Instant,
};

use ash::vk;
use ring_buffer::RingBuffer;
use tracing::{debug, span, Level};
use winit::event_loop;

use crate::{
    analysis::Analysis,
    error::{Error, VResult},
    ring_buffer,
    utils::sleep_ms,
    vulkan::{self, multi_buffer, multi_image, Vulkan},
    window::Window,
    Args,
};

#[derive(Debug)]
struct PushConstants(HashMap<String, vulkan::Value>);

impl Deref for PushConstants {
    type Target = HashMap<String, vulkan::Value>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PushConstants {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PushConstants {
    fn new() -> Self {
        Self(HashMap::new())
    }

    fn bool(&mut self, name: &str, bool: bool) {
        self.insert(name.to_owned(), vulkan::Value::Bool(bool));
    }

    fn u32(&mut self, name: &str, u32: u32) {
        self.insert(name.to_owned(), vulkan::Value::U32(u32));
    }

    fn f32(&mut self, name: &str, f32: f32) {
        self.insert(name.to_owned(), vulkan::Value::F32(f32));
    }
}

pub struct Visualizer {
    bass_signal_gpu: Rc<multi_buffer::MultiBuffer>,
    signal_dft_gpu: Rc<multi_buffer::MultiBuffer>,

    new_resolution: Option<vk::Extent2D>,
    last_resized_time: Instant,

    // These should be dropped last.
    images: Vec<Rc<multi_image::MultiImage>>,
    vulkan: Vulkan,
}

impl Visualizer {
    fn reinitialize_images(&mut self) -> VResult<()> {
        // Drop old images.
        self.images.clear();

        let vulkan = &mut self.vulkan;
        let image_size = vulkan.surface_info.surface_resolution;

        let canvas = vulkan.new_multi_image("canvas", image_size, None)?;
        self.images.push(canvas);
        let accent = vulkan.new_multi_image("accent", image_size, None)?;
        self.images.push(accent);

        let frame = vulkan.new_multi_image("frame", image_size, None)?;
        let frame_prev = vulkan.prev_shift(&frame, "frame_prev");

        self.images.push(frame);
        self.images.push(frame_prev);

        Ok(())
    }

    pub fn new(
        args: &Args,
        signal: &RingBuffer<f32>,
        analysis: &Analysis,
    ) -> VResult<(event_loop::EventLoop<()>, Visualizer)> {
        let (event_loop, window) = Window::new()?;
        let window = Rc::new(window);
        let mut vulkan = Vulkan::new(&window, &args.shader_paths, !args.no_vsync)?;

        let bass_signal_gpu = {
            let size = signal.serialized_size();
            vulkan.new_multi_buffer("bass_signal", size, Some(1))?
        };
        // let low_pass_gpu = {
        //     let size = analysis.low_pass_buffer.serialized_size();
        //     vulkan.new_multi_buffer("low_pass", size, Some(1))?
        // };
        // let high_pass_gpu = {
        //     let size = analysis.high_pass_buffer.serialized_size();
        //     vulkan.new_multi_buffer("high_pass", size, Some(1))?
        // };
        let signal_dft_gpu = {
            let size = analysis.signal_dft.log_bin_serialized_size();
            vulkan.new_multi_buffer("signal_dft", size, Some(1))?
        };
        // let low_pass_dft_gpu = {
        //     let size = analysis.low_pass_dft.serialized_size();
        //     vulkan.new_multi_buffer("low_pass_dft", size, Some(1))?
        // };
        // let high_pass_dft_gpu = {
        //     let size = analysis.high_pass_dft.serialized_size();
        //     vulkan.new_multi_buffer("high_pass_dft", size, Some(1))?
        // };

        let mut visualizer = Self {
            bass_signal_gpu,
            signal_dft_gpu,
            // low_pass_gpu,
            // low_pass_dft_gpu,
            // high_pass_gpu,
            // high_pass_dft_gpu,
            new_resolution: None,
            last_resized_time: Instant::now(),
            images: Vec::new(),
            vulkan,
        };

        visualizer.reinitialize_images()?;
        Ok((event_loop, visualizer))
    }

    pub fn debounce_resize(&mut self, width: u32, height: u32) {
        let current = self.vulkan.surface_info.surface_resolution;
        if width == current.width && height == current.height {
            self.new_resolution = None;
            return;
        }

        if self.new_resolution.is_none() {
            debug!("Debouncing resize event");
        }
        self.last_resized_time = Instant::now();
        self.new_resolution = Some(vk::Extent2D { width, height })
    }

    fn exec_resize(&mut self) -> VResult<()> {
        let span = span!(Level::INFO, "Visualizer::exec_resize");
        let _span_guard = span.enter();

        self.new_resolution = None;

        unsafe {
            self.vulkan.reinitialize_swapchain()?;
        }

        // Use w/h?
        self.reinitialize_images()?;
        Ok(())
    }

    pub fn tick(&mut self, analysis: &Analysis) -> VResult<()> {
        if self.new_resolution.is_some() {
            // Don't render anything.
            sleep_ms(16);

            if self.last_resized_time.elapsed().as_secs_f32() > 0.05 {
                self.exec_resize().expect("tickresize");
            }

            return Ok(());
        }

        let read_index = analysis.tick_start_index;
        let write_index = analysis.tick_end_index;

        analysis
            .signal_dft
            .write_log_bins_to_pointer(self.signal_dft_gpu.mapped(0));

        analysis.bass_energy.write_to_pointer(
            read_index,
            write_index,
            self.bass_signal_gpu.mapped(0),
        );

        // Collect invocation constants.
        let mut push_constants = PushConstants::new();

        push_constants.u32("frame_index", self.vulkan.num_frames as u32);
        push_constants.f32("time", analysis.epoch.elapsed().as_secs_f32());

        let bass = &analysis.beat_detector.energy;
        push_constants.f32("bass_energy", bass.last());
        push_constants.f32("cumulative_bass_energy", bass.cumulative());

        push_constants.bool("is_beat", analysis.beat_in_tick);
        push_constants.u32("real_beats", analysis.real_beats);

        let confidence = analysis.bpm_tracker.bpm_confidence();
        push_constants.f32("bpm_confidence", confidence);
        push_constants.f32("bpm_period", analysis.bpm_tracker.bpm.period);
        push_constants.u32("beat_index", analysis.fake_beats);
        push_constants.f32("beat_fract", analysis.beat_fract);

        // Actually render sth.
        if let Err(Error::Vk(vk::Result::ERROR_OUT_OF_DATE_KHR)) =
            unsafe { self.vulkan.tick(&push_constants) }
        {
            self.debounce_resize(0, 0);
        };

        Ok(())
    }
}

impl Drop for Visualizer {
    fn drop(&mut self) {
        self.vulkan.wait_idle();
    }
}
