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

    // low_pass_gpu: Rc<multi_buffer::MultiBuffer>,
    // low_pass_dft_gpu: Rc<multi_buffer::MultiBuffer>,
    //
    // high_pass_gpu: Rc<multi_buffer::MultiBuffer>,
    // high_pass_dft_gpu: Rc<multi_buffer::MultiBuffer>,

    // let dft_result_size = Dft::output_byte_size(args.dft_size) + mem::size_of::<i32>();
    // history: History::new(history_size),
    // history_gpu: vulkan.new_multi_buffer("history", history_gpu_size, Some(1))?,
    // // Averages.
    // long_avg: AlphaAvg::new(0.99),
    // short_avg: WindowedAvg::new((0.2 * frame_rate as f32) as usize),
    // // Beat detection.
    // noise_threshold_factor: 0.25,
    // beat_sigma_threshold_factor: 2.2,
    // is_high: false,
    // is_beat: false,
    // // BPM detection.
    // autocorrelation: Dft::new(8 * frame_rate),
    // autocorrelation_gpu: vulkan.new_multi_buffer(
    //     "autocorrelation",
    //     autocorrelation_gpu_size,
    //     Some(1),
    // )?,
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

        // let intermediate_prev = vulkan.prev_shift(&intermediate, "intermediate_prev");
        // self.images.push(intermediate);
        // self.images.push(intermediate_prev);

        // let highlights = vulkan.new_multi_image("highlights", image_size, None)?;
        // self.images.push(highlights);
        // let bloom_h = vulkan.new_multi_image("bloom_h", image_size, None)?;
        // self.images.push(bloom_h);
        // let bloom_hv = vulkan.new_multi_image("bloom_hv", image_size, None)?;
        // self.images.push(bloom_hv);
        // let result = vulkan.new_multi_image("result", image_size, None)?;
        // let result_prev = vulkan.prev_shift(&result, "result_prev");
        // self.images.push(result);
        // self.images.push(result_prev);

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

        // signal.write_to_pointer(read_index, write_index, self.signal_gpu.mapped(0));
        // analysis.low_pass_buffer.write_to_pointer(
        //     read_index,
        //     write_index,
        //     self.low_pass_gpu.mapped(0),
        // );
        // analysis.high_pass_buffer.write_to_pointer(
        //     read_index,
        //     write_index,
        //     self.high_pass_gpu.mapped(0),
        // );
        //
        analysis
            .signal_dft
            .write_log_bins_to_pointer(self.signal_dft_gpu.mapped(0));

        analysis.beat_detector.bass_buffer.write_to_pointer(
            read_index,
            write_index,
            self.bass_signal_gpu.mapped(0),
        );
        // analysis
        //     .low_pass_dft
        //     .write_to_pointer(self.low_pass_dft_gpu.mapped(0));
        // analysis
        //     .high_pass_dft
        //     .write_to_pointer(self.high_pass_dft_gpu.mapped(0));

        let mut push_constants = PushConstants::new();

        push_constants.u32("frame_index", self.vulkan.num_frames as u32);
        push_constants.f32("time", analysis.epoch.elapsed().as_secs_f32());

        let bass_energy = &analysis.beat_detector.bass_energy;
        push_constants.f32("bass_energy", bass_energy.frame_energy);
        push_constants.f32("cumulative_bass_energy", bass_energy.cumulative_bass_energy);
        push_constants.bool("is_beat", analysis.beat_in_tick);
        push_constants.u32("real_beats", analysis.real_beats);

        let confidence = analysis.bpm_tracker.bpm_confidence();
        push_constants.f32("bpm_confidence", confidence);
        push_constants.f32("bpm_period", analysis.bpm_tracker.bpm.period);
        push_constants.u32("beat_index", analysis.fake_beats);
        push_constants.f32("beat_fract", analysis.beat_fract);

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
