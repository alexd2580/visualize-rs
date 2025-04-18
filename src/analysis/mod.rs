pub mod beat_detector;
pub mod bpm_tracker;
pub mod dft;
pub mod server;

use std::{
    sync::Arc,
    time::{self, Instant},
};

use beat_detector::BeatDetector;
use bpm_tracker::BpmTracker;
use dft::Dft;
use server::FrameSender;

use crate::{
    filters::{filter::Filter, max_decay_normalizer::MaxDecayNormalizer},
    ring_buffer::RingBuffer,
    Args,
};

/// Note the reverse drop order.
pub struct Analysis {
    sample_rate: f32,
    pub buf_size: usize,

    pub epoch: time::Instant,
    last_tick: time::Instant,
    pub sample_index: u64,

    pub tick_start_index: usize,
    pub tick_end_index: usize,

    normalizer: MaxDecayNormalizer,

    signal: RingBuffer<f32>,
    pub signal_dft: Dft,

    pub beat_detector: BeatDetector,
    pub bpm_tracker: BpmTracker,

    pub beat_in_tick: bool,
    pub real_beats: u32,

    pub fake_beats: u32,
    pub beat_fract: f32,

    // pub low_pass: LowPass,
    // pub low_pass_buffer: RingBuffer<f32>,
    // pub low_pass_dft: Dft,

    // pub high_pass: HighPass,
    // pub high_pass_buffer: RingBuffer<f32>,
    // pub high_pass_dft: Dft,

    //
    // pub frequency_band_border_indices: [usize; 8],
    //
    // pub beat_dft_range: (usize, usize),
    // pub beat_detectors: Vec<BeatDetection>,
    broadcast: Option<Arc<FrameSender>>,
}

impl Analysis {
    pub fn new(args: &Args, sample_rate: f32, broadcast: Option<Arc<FrameSender>>) -> Self {
        let audio_buffer_size = (args.audio_buffer_sec * sample_rate) as usize;

        let dft_size = args.dft_size;

        // let dft_window_per_s = sample_rate as f32 / dft_size as f32;
        // let dft_min_fq = dft_window_per_s * 1f32;
        // let dft_max_fq = dft_window_per_s * dft_size as f32 / 2f32;
        // info!("DFT can analyze frequencies in the range: {dft_min_fq} hz - {dft_max_fq} hz");

        // let frequency_band_borders = [16, 60, 250, 500, 2000, 4000, 6000, 22000];
        // let frequency_band_border_indices = frequency_band_borders
        //     .map(|frequency| dft_index_of_frequency(frequency, audio.sample_rate(), dft_size));
        //
        // let beat_dft_lower = dft_index_of_frequency(35, audio.sample_rate(), dft_size);
        // let beat_dft_upper = dft_index_of_frequency(125, audio.sample_rate(), dft_size);

        Self {
            sample_rate,
            buf_size: audio_buffer_size,

            epoch: Instant::now(),
            last_tick: Instant::now(),
            sample_index: 0,

            tick_start_index: 0,
            tick_end_index: 0,

            normalizer: MaxDecayNormalizer::new(0.999997, 0.05),
            signal: RingBuffer::new(audio_buffer_size),
            signal_dft: Dft::new(dft_size, sample_rate),

            beat_detector: BeatDetector::new(args, sample_rate),
            bpm_tracker: BpmTracker::new(args, sample_rate),

            beat_in_tick: false,
            real_beats: 0,

            fake_beats: 0,
            beat_fract: 0.0,

            // low_pass: LowPass::new(sample_rate, 100),
            // low_pass_buffer: RingBuffer::new(audio_buffer_size),
            // low_pass_dft: Dft::new(args.dft_size),
            // high_pass: HighPass::new(sample_rate, 100),
            // high_pass_buffer: RingBuffer::new(audio_buffer_size),
            // high_pass_dft: Dft::new(args.dft_size),
            broadcast,
        }
    }

    /// Compute the read index (start of data to read), write index (index at which new data will
    /// be written (end of data to read).
    fn update_slice_indices(&mut self, signal: &RingBuffer<f32>, delta: f32) {
        // Requiring self.signal and signal to be of same size.
        let index_first_new = self.signal.write_index;
        let index_last_new = signal.write_index;

        // Total available samples - samples that haven't been copied from `signal` to `self.signal`.
        let available_samples = if index_last_new < index_first_new {
            index_last_new + self.buf_size - index_first_new
        } else {
            index_last_new - index_first_new
        };

        // I want to consume this much!
        let mut consume_samples = (self.sample_rate * delta) as usize + 5;

        if consume_samples > available_samples {
            // Don't care about underruns...
            // let underrun = consume_samples - available_samples;
            // warn!("Sample underrun by {underrun}");
            consume_samples = available_samples;
        }

        self.tick_start_index = index_first_new;
        self.tick_end_index = (index_first_new + consume_samples) % self.buf_size;
    }

    fn on_pcm_sample(&mut self, x: f32) {
        self.sample_index += 1;

        let x = self.normalizer.sample(x);
        self.signal.push(x);

        if self.beat_detector.on_pcm_sample(self.sample_index, x) {
            self.real_beats += 1;
            self.beat_in_tick = true;
            self.bpm_tracker.on_beat(self.sample_index);
        }

        // Every 128th PCM sample.
        if self.sample_index & 0b1111111 == 0 {
            let to_float = |x: bool| if x { 1.0 } else { 0.0 };
            if let Some(broadcast) = &self.broadcast {
                broadcast
                    .send(vec![
                        self.beat_detector.bass_stats.energy,
                        self.beat_detector.bass_stats.short.avg,
                        self.beat_detector.bass_stats.long.avg,
                        to_float(self.beat_in_tick),
                        self.bpm_tracker.beat_probability(self.sample_index),
                        self.bpm_tracker.phase_error / 50.0 + 0.5,
                    ])
                    .expect("Failed to broadcast frame bass frequencies");
            }
        }
    }

    // A tick is @ 60Hz / or so i think...
    // A sample is @ 44100Hz
    // A frame is @ 44100Hz / 64 == 689.0625Hz
    pub fn on_tick(&mut self, signal: &RingBuffer<f32>) {
        let delta = self.last_tick.elapsed().as_secs_f32();
        self.last_tick = Instant::now();

        self.beat_in_tick = false;
        let fract_pre = self.beat_fract;

        // Run sample-by-sample analysis.
        self.update_slice_indices(signal, delta);
        let start = self.tick_start_index;
        let end = self.tick_end_index;

        if end < start {
            for index in (start..self.buf_size).chain(0..end) {
                self.on_pcm_sample(signal.data[index]);
            }
        } else {
            for index in start..end {
                self.on_pcm_sample(signal.data[index]);
            }
        }

        self.beat_fract = self.bpm_tracker.sample_to_beat_fract(self.sample_index);
        if self.beat_fract < 0.1 && fract_pre > 0.9 {
            self.fake_beats += 1;
        }

        // Run DFTs on filtered/split signals.
        let samples_since_last_multiple_of_dft = self.sample_index & 0b11111111;
        let offset_from_end = samples_since_last_multiple_of_dft as usize + self.signal_dft.size();
        let dft_vec = self.signal_dft.get_input_vec();
        self.signal.write_to_buffer(offset_from_end, dft_vec);
        self.signal_dft.run_transform();

        // let low_pass_vec = self.low_pass_dft.get_input_vec();
        // self.low_pass_buffer.write_to_buffer(low_pass_vec);
        // self.low_pass_dft.run_transform();
        //
        // let high_pass_vec = self.high_pass_dft.get_input_vec();
        // self.high_pass_buffer.write_to_buffer(high_pass_vec);
        // self.high_pass_dft.run_transform();
    }
}
