use std::{mem, rc::Rc, time};

use ash::vk;
use clap::Parser;

use error::Error;
use log::{error, info, warn};

mod audio;
mod dft;
mod error;
mod ring_buffer;
mod thread_shared;
mod timer;
mod utils;
mod vulkan;
mod window;

struct BeatAnalysis {
    noise_threshold_factor: f32,
    beat_sigma_threshold_factor: f32,

    last_values: Vec<f32>,
    write_index: usize,

    long_sum_size: usize,
    long_sum: f32,

    short_sum_size: usize,
    short_sum: f32,
    square_sum: f32,

    standard_deviation: f32,

    // Whether there is currentle an extraordinary signal energy.
    is_beat: bool,
    // Absolute beat count.
    beat_count: usize,

    // Timestamps of the last N detected beats.
    beat_timestamps: Vec<time::Instant>,
    // Estimated BPM, but in seconds-per-beat format.
    spb: f32,
}

fn wrap_index(pos_offset: usize, neg_offset: usize, len: usize) -> usize {
    let idx = pos_offset + len - neg_offset;
    if idx >= len {
        idx % len
    } else {
        idx
    }
}

impl BeatAnalysis {
    fn new(short_avg_size: usize, long_avg_size: usize) -> Self {
        let beat_timestamps = (0..30)
            .rev()
            .map(|n| {
                time::Instant::now()
                    .checked_sub(time::Duration::from_secs(n))
                    .unwrap()
            })
            .collect::<Vec<_>>();
        Self {
            noise_threshold_factor: 0.25,
            beat_sigma_threshold_factor: 2.2,
            last_values: vec![0f32; long_avg_size],
            write_index: 0,
            long_sum_size: long_avg_size,
            long_sum: 0f32,
            short_sum_size: short_avg_size,
            short_sum: 0f32,
            square_sum: 0f32,
            standard_deviation: 0f32,
            is_beat: false,
            beat_count: 0,
            beat_timestamps,
            spb: 0.1666,
        }
    }

    fn sample(&mut self, x: f32) {
        let buf_len = self.last_values.len();

        let long_sum_read_index = wrap_index(self.write_index, self.long_sum_size, buf_len);
        let long_sum_read_value = self.last_values[long_sum_read_index];
        self.long_sum = self.long_sum - long_sum_read_value + x;
        let long_avg = self.long_sum / self.long_sum_size as f32;

        let short_sum_read_index = wrap_index(self.write_index, self.short_sum_size, buf_len);
        let short_sum_read_value = self.last_values[short_sum_read_index];
        self.short_sum = self.short_sum - short_sum_read_value + x;
        let short_avg = self.short_sum / self.short_sum_size as f32;

        let square_sum_read_value = short_sum_read_value.powf(2f32);
        self.square_sum = self.square_sum - square_sum_read_value + x.powf(2f32);
        let square_avg = self.square_sum / self.short_sum_size as f32;

        self.standard_deviation = (square_avg - short_avg.powf(2f32)).sqrt();

        self.last_values[self.write_index] = x;
        self.write_index = wrap_index(self.write_index + 1, 0, buf_len);

        let not_noise = short_avg > self.noise_threshold_factor * long_avg;
        let loud_outlier =
            x > short_avg + self.beat_sigma_threshold_factor * self.standard_deviation;

        let is_beat = not_noise && loud_outlier;
        // Register the beat on the raising edge.
        if !self.is_beat && is_beat {
            let len = self.beat_timestamps.len();
            let idx = wrap_index(self.beat_count, 0, len);
            self.beat_timestamps[idx] = time::Instant::now();
            self.beat_count += 1;

            // Group beat deltas into clusters.
            let mut clusters: Vec<(f32, usize)> = Vec::new();
            for index in 0..(len - 1) {
                let a = self.beat_timestamps[wrap_index(self.beat_count + index, 0, len)];
                let b = self.beat_timestamps[wrap_index(self.beat_count + index + 1, 0, len)];
                let delta = (b - a).as_secs_f32();

                if let Some((sum, count)) = clusters.iter_mut().find(|(sum, count)| {
                    let center = *sum / *count as f32;
                    let relative_dist = ((center / delta) - 1f32).abs();
                    relative_dist < 0.1
                }) {
                    // Vote for this cluster.
                    *sum += delta;
                    *count += 1;
                } else {
                    // This branch is reached if no cluster matches.
                    clusters.push((delta, 1));
                }
            }

            let (spb_sum, count) =
                clusters
                    .into_iter()
                    .fold((f32::INFINITY, 0), |a, b| if a.1 > b.1 { a } else { b });

            self.spb = spb_sum / count as f32;
        }
        self.is_beat = is_beat;

        // debug!(
        //     "X:{:>5.2} S:{:>5.2} L:{:>5.2} SQ:{:>5.2} SD:{:>5.2} {} {}",
        //     x * 100f32,
        //     short_avg * 100f32,
        //     long_avg * 100f32,
        //     square_avg * 100f32,
        //     self.standard_deviation * 100f32,
        //     not_noise,
        //     loud_outlier
        // );
    }

    fn last_beat(&self) -> time::Instant {
        self.beat_timestamps[wrap_index(self.beat_count, 1, self.beat_timestamps.len())]
    }

    fn next_beat(&self) -> time::Instant {
        self.last_beat() + time::Duration::from_secs_f32(self.spb)
    }
}

/// Note the reverse drop order.
struct Visualizer {
    epoch: time::Instant,

    available_samples: usize,
    avg_available_samples: f32,
    avg_available_samples_alpha: f32,

    _frequency_band_border_indices: [usize; 8],
    beat_analysis: BeatAnalysis,

    audio: audio::Audio,
    signal_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,
    signal_dft: dft::Dft,
    signal_dft_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,

    low_pass: audio::low_pass::LowPass,
    low_pass_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,
    low_pass_dft: dft::Dft,
    low_pass_dft_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,

    high_pass: audio::high_pass::HighPass,
    high_pass_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,
    high_pass_dft: dft::Dft,
    high_pass_dft_gpu: Rc<vulkan::multi_buffer::MultiBuffer>,

    images: Vec<Rc<vulkan::multi_image::MultiImage>>,

    vulkan: vulkan::Vulkan,

    timer: timer::Timer,
}

fn dft_index_of_frequency(frequency: usize, sample_rate: usize, dft_size: usize) -> usize {
    // For reference see
    // https://stackoverflow.com/questions/4364823/how-do-i-obtain-the-frequencies-of-each-value-in-an-fft
    // 0:   0 * 44100 / 1024 =     0.0 Hz
    // 1:   1 * 44100 / 1024 =    43.1 Hz
    // 2:   2 * 44100 / 1024 =    86.1 Hz
    // 3:   3 * 44100 / 1024 =   129.2 Hz
    (frequency as f32 * dft_size as f32 / sample_rate as f32).round() as usize
}

impl Visualizer {
    fn run_dft(
        buffer: &ring_buffer::RingBuffer<f32>,
        dft: &mut dft::Dft,
        dft_gpu: &vulkan::multi_buffer::MultiBuffer,
    ) {
        buffer.write_to_buffer(dft.get_input_vec());
        dft.run_transform();
        dft.write_to_pointer(dft_gpu.mapped(0));
    }

    fn reinitialize_images(&mut self) -> Result<(), error::Error> {
        // Drop old images.
        self.images.clear();

        let vulkan = &mut self.vulkan;
        let image_size = vulkan.surface_info.surface_resolution;

        let intermediate = vulkan.new_multi_image("intermediate", image_size, None)?;
        let intermediate_prev = vulkan.prev_shift(&intermediate, "intermediate_prev");
        self.images.push(intermediate);
        self.images.push(intermediate_prev);

        let highlights = vulkan.new_multi_image("highlights", image_size, None)?;
        self.images.push(highlights);
        let bloom_h = vulkan.new_multi_image("bloom_h", image_size, None)?;
        self.images.push(bloom_h);
        let bloom_hv = vulkan.new_multi_image("bloom_hv", image_size, None)?;
        self.images.push(bloom_hv);
        let result = vulkan.new_multi_image("result", image_size, None)?;
        let result_prev = vulkan.prev_shift(&result, "result_prev");
        self.images.push(result);
        self.images.push(result_prev);

        Ok(())
    }

    fn new(window: &window::Window, args: &Args) -> Result<Visualizer, Error> {
        let mut vulkan = vulkan::Vulkan::new(window, &args.shader_paths, args.vsync)?;
        let images = Vec::new();

        // TODO dynamic?
        let frame_rate = 60;

        let audio = audio::Audio::new(args.audio_buffer_sec, false)?;
        let audio_buffer_size = audio.buffer_size();
        let audio_buffer_bytes = audio_buffer_size * mem::size_of::<f32>();
        let signal_gpu = vulkan.new_multi_buffer("signal", audio_buffer_bytes, Some(1))?;

        let low_pass = audio::low_pass::LowPass::new(audio_buffer_size, 0.02);
        let low_pass_gpu = vulkan.new_multi_buffer("low_pass", audio_buffer_bytes, Some(1))?;

        let high_pass = audio::high_pass::HighPass::new(audio_buffer_size, 0.1);
        let high_pass_gpu = vulkan.new_multi_buffer("high_pass", audio_buffer_bytes, Some(1))?;

        let dft_size = args.dft_size;
        let dft_window_per_s = audio.sample_rate as f32 / dft_size as f32;
        let dft_min_fq = dft_window_per_s * 1f32;
        let dft_max_fq = dft_window_per_s * dft_size as f32 / 2f32;
        info!("DFT can analyze frequencies in the range: {dft_min_fq} hz - {dft_max_fq} hz");

        let frequency_band_borders = [16, 60, 250, 500, 2000, 4000, 6000, 22000];
        let frequency_band_border_indices = frequency_band_borders
            .map(|frequency| dft_index_of_frequency(frequency, audio.sample_rate, dft_size));

        let dft_result_size = dft::Dft::output_byte_size(args.dft_size);

        let signal_dft = dft::Dft::new(args.dft_size);
        let signal_dft_gpu = vulkan.new_multi_buffer("signal_dft", dft_result_size, Some(1))?;

        let low_pass_dft = dft::Dft::new(args.dft_size);
        let low_pass_dft_gpu = vulkan.new_multi_buffer("low_pass_dft", dft_result_size, Some(1))?;

        let high_pass_dft = dft::Dft::new(args.dft_size);
        let high_pass_dft_gpu =
            vulkan.new_multi_buffer("high_pass_dft", dft_result_size, Some(1))?;

        let short_size = (0.2 * frame_rate as f32) as usize;
        let long_size = 4 * frame_rate;
        let beat_analysis = BeatAnalysis::new(short_size, long_size);

        let mut visualizer = Self {
            epoch: time::Instant::now(),
            timer: timer::Timer::new(0.9),
            available_samples: 0,
            avg_available_samples: 44100f32 / 60f32,
            avg_available_samples_alpha: 0.95,
            audio,
            signal_gpu,
            signal_dft,
            signal_dft_gpu,
            low_pass,
            low_pass_gpu,
            low_pass_dft,
            low_pass_dft_gpu,
            high_pass,
            high_pass_gpu,
            high_pass_dft,
            high_pass_dft_gpu,
            _frequency_band_border_indices: frequency_band_border_indices,
            beat_analysis,
            images,
            vulkan,
        };

        visualizer.reinitialize_images()?;
        Ok(visualizer)
    }

    /// Returns the read index (start of data to read), write index (index at which new data will
    /// be written (end of data to read) and the size of the ring buffer.
    fn data_indices(&mut self) -> (usize, usize, usize) {
        let read_index = self.low_pass.write_index;
        let write_index = self.audio.left.write_index;
        let buf_size = self.audio.left.data.len();

        // Total available samples.
        let available_samples = if write_index < read_index {
            write_index + buf_size - read_index
        } else {
            write_index - read_index
        };

        // New available in this frame.
        let new_available = available_samples - self.available_samples;
        self.avg_available_samples = self.avg_available_samples * self.avg_available_samples_alpha
            + new_available as f32 * (1f32 - self.avg_available_samples_alpha);

        // `+5` makes it so that i try to display more frames without lagging behind too much.
        // This is a magic number, might be different for different FPS.
        let mut consume_samples = self.avg_available_samples as usize + 2;
        let (sample_underrun, ok) = consume_samples.overflowing_sub(available_samples);
        let sample_underrun_pct = 100f32 * sample_underrun as f32 / consume_samples as f32;
        if !ok && consume_samples > available_samples {
            if sample_underrun_pct > 50f32 {
                warn!("Sample underrun by {sample_underrun} ({sample_underrun_pct:.2}%)");
            }
            consume_samples = available_samples;
        }

        let sample_overrun_pct =
            100f32 * available_samples as f32 / (consume_samples as f32 + 1f32);
        if ok && sample_overrun_pct > 2000f32 {
            warn!("Sample overrun by {available_samples} ({sample_overrun_pct:.2}%)");
        }

        self.available_samples = available_samples - consume_samples;

        let write_index = (read_index + consume_samples) % buf_size;

        (read_index, write_index, buf_size)
    }

    fn run_vulkan(&mut self) -> Result<(), Error> {
        let mut push_constant_values = std::collections::HashMap::new();
        push_constant_values.insert(
            "is_beat".to_owned(),
            vulkan::Value::Bool(self.beat_analysis.is_beat),
        );
        push_constant_values.insert(
            "beat_count".to_owned(),
            vulkan::Value::U32(u32::try_from(self.beat_analysis.beat_count).unwrap()),
        );
        push_constant_values.insert(
            "now".to_owned(),
            vulkan::Value::F32(self.epoch.elapsed().as_secs_f32()),
        );
        let last_beat = (self.beat_analysis.last_beat() - self.epoch).as_secs_f32();
        push_constant_values.insert("last_beat".to_owned(), vulkan::Value::F32(last_beat));
        let next_beat = (self.beat_analysis.next_beat() - self.epoch).as_secs_f32();
        push_constant_values.insert("next_beat".to_owned(), vulkan::Value::F32(next_beat));

        match unsafe { self.vulkan.tick(&push_constant_values)? } {
            None => (),
            Some(vulkan::Event::Resized) => self.reinitialize_images()?,
        }
        Ok(())
    }

    fn tick(&mut self) -> winit::event_loop::ControlFlow {
        self.timer.section("Outside of loop");

        let (read_index, write_index, buf_size) = self.data_indices();

        if write_index < read_index {
            for index in read_index..buf_size {
                let x = self.audio.left.data[index];
                self.low_pass.sample(x);
                self.high_pass.sample(x);
            }
            for index in 0..write_index {
                let x = self.audio.left.data[index];
                self.low_pass.sample(x);
                self.high_pass.sample(x);
            }
        } else {
            for index in read_index..write_index {
                let x = self.audio.left.data[index];
                self.low_pass.sample(x);
                self.high_pass.sample(x);
            }
        }

        self.timer.section("Filters");

        self.audio
            .left
            .write_to_pointer(read_index, write_index, self.signal_gpu.mapped(0));

        self.low_pass
            .write_to_pointer(read_index, write_index, self.low_pass_gpu.mapped(0));

        self.high_pass
            .write_to_pointer(read_index, write_index, self.high_pass_gpu.mapped(0));

        self.timer.section("Filters to GPU");

        Self::run_dft(&self.audio.left, &mut self.signal_dft, &self.signal_dft_gpu);

        Self::run_dft(
            &self.low_pass,
            &mut self.low_pass_dft,
            &self.low_pass_dft_gpu,
        );

        Self::run_dft(
            &self.high_pass,
            &mut self.high_pass_dft,
            &self.high_pass_dft_gpu,
        );

        let beat_dft = &self.low_pass_dft;
        let beat_dft_lower = dft_index_of_frequency(35, self.audio.sample_rate, beat_dft.size());
        let beat_dft_upper = dft_index_of_frequency(125, self.audio.sample_rate, beat_dft.size());
        let beat_dft_sum_size = beat_dft_upper - beat_dft_lower;
        let beat_dft_sum = beat_dft.simple[beat_dft_lower..beat_dft_upper]
            .iter()
            .fold(0f32, |a, b| a + b);
        self.beat_analysis
            .sample(beat_dft_sum / beat_dft_sum_size as f32);

        self.timer.section("DFTs and DFTs to GPU");

        let result = match self.run_vulkan() {
            Ok(()) => winit::event_loop::ControlFlow::Poll,
            Err(err) => {
                error!("{err}");
                winit::event_loop::ControlFlow::ExitWithCode(1)
            }
        };

        self.timer.section("Vulkan");

        if self.vulkan.num_frames % 600 == 0 {
            self.timer.print();
        }

        result
    }
}

impl Drop for Visualizer {
    fn drop(&mut self) {
        self.vulkan.wait_idle();
    }
}

impl window::App for Visualizer {
    fn loop_body(&mut self) -> winit::event_loop::ControlFlow {
        self.tick()
    }
}

/// Run an audio visualizer.
#[derive(Parser, Debug)]
struct Args {
    /// The shader module path
    #[arg(short, long, num_args = 0.., default_value = "shaders/debug.comp")]
    shader_paths: Vec<std::path::PathBuf>,

    /// The DFT size
    #[arg(short, long, default_value = "2048")]
    dft_size: usize,

    /// The audio buffer size
    #[arg(short, long, default_value = "4")]
    audio_buffer_sec: u32,

    /// Enable vsync
    #[arg(short, long, default_value = "true", action = clap::ArgAction::Set)]
    vsync: bool,
}

fn run_main(args: &Args) -> Result<(), Error> {
    let mut window = window::Window::new()?;

    {
        let mut visualizer = Visualizer::new(&window, args)?;
        log::info!("Running...");
        window.run_main_loop(&mut visualizer);
    }

    Ok(())
}

fn main() {
    simple_logger::init_with_level(log::Level::Debug).unwrap();
    log::info!("Initializing...");
    let args = Args::parse();
    if let Err(err) = run_main(&args) {
        error!("{}", err);
    }
    log::info!("Terminating...");
}
