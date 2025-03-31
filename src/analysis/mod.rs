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
    pub epoch: time::Instant,
    pub samples_processed: u64,

    pub read_index: usize,
    pub write_index: usize,
    pub buf_size: usize,

    available_samples: usize,
    avg_available_samples: f32,
    avg_available_samples_alpha: f32,

    normalizer: MaxDecayNormalizer,

    signal: RingBuffer<f32>,
    pub signal_dft: Dft,

    beat_detector: BeatDetector,
    bpm_tracker: BpmTracker,

    beat_in_tick: bool,

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
    const BPM_FRAMES_PER_SAMPLE: f32 = 64.0;

    pub fn new(args: &Args, sample_rate: f32, broadcast: Option<Arc<FrameSender>>) -> Self {
        let audio_buffer_size = (args.audio_buffer_sec * sample_rate) as usize;

        let dft_size = args.dft_size;

        // let dft_window_per_s = sample_rate as f32 / dft_size as f32;
        // let dft_min_fq = dft_window_per_s * 1f32;
        // let dft_max_fq = dft_window_per_s * dft_size as f32 / 2f32;
        // log::info!("DFT can analyze frequencies in the range: {dft_min_fq} hz - {dft_max_fq} hz");

        // let frequency_band_borders = [16, 60, 250, 500, 2000, 4000, 6000, 22000];
        // let frequency_band_border_indices = frequency_band_borders
        //     .map(|frequency| dft_index_of_frequency(frequency, audio.sample_rate(), dft_size));
        //
        // let beat_dft_lower = dft_index_of_frequency(35, audio.sample_rate(), dft_size);
        // let beat_dft_upper = dft_index_of_frequency(125, audio.sample_rate(), dft_size);

        Self {
            epoch: Instant::now(),
            samples_processed: 0,

            read_index: 0,
            write_index: 0,
            buf_size: 0,
            available_samples: 0,
            avg_available_samples: 44100f32 / 60f32,
            avg_available_samples_alpha: 0.95,

            normalizer: MaxDecayNormalizer::new(0.9999, 0.05),
            signal: RingBuffer::new(audio_buffer_size),
            signal_dft: Dft::new(dft_size),

            beat_detector: BeatDetector::new(args, sample_rate),
            bpm_tracker: BpmTracker::new(args, sample_rate),
            beat_in_tick: false,

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
    /// be written (end of data to read) and the size of the ring buffer.
    fn compute_data_indices(&mut self, signal: &RingBuffer<f32>) -> (usize, usize, usize) {
        let read_index = self.signal.write_index;
        let write_index = signal.write_index;
        let buf_size = signal.data.len();

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

        // `+ n` makes it so that i try to display more frames without lagging behind too much.
        // This is a magic number, might be different for different FPS.
        let mut consume_samples = self.avg_available_samples as usize + 2;
        let (sample_underrun, ok) = consume_samples.overflowing_sub(available_samples);
        let sample_underrun_pct = 100f32 * sample_underrun as f32 / consume_samples as f32;
        if !ok && consume_samples > available_samples {
            if sample_underrun_pct > 50f32 {
                log::warn!("Sample underrun by {sample_underrun} ({sample_underrun_pct:.2}%)");
            }
            consume_samples = available_samples;
        }

        let sample_overrun_pct =
            100f32 * available_samples as f32 / (consume_samples as f32 + 1f32);
        if ok && sample_overrun_pct > 2000f32 {
            log::warn!("Sample overrun by {available_samples} ({sample_overrun_pct:.2}%)");
        }

        self.available_samples = available_samples - consume_samples;

        let write_index = (read_index + consume_samples) % buf_size;

        self.read_index = read_index;
        self.write_index = write_index;
        self.buf_size = buf_size;

        (read_index, write_index, buf_size)
    }

    fn on_pcm_sample(&mut self, x: f32) {
        self.samples_processed += 1;

        let x = self.normalizer.sample(x);
        self.signal.push(x);

        if self.beat_detector.on_pcm_sample(self.samples_processed, x) {
            self.beat_in_tick = true;
            self.bpm_tracker.on_beat(self.samples_processed);
        }

        // Every 64th PCM sample.
        if self.samples_processed & 0b111111 == 0 {
            let to_float = |x: bool| if x { 1.0 } else { 0.0 };
            if let Some(broadcast) = &self.broadcast {
                broadcast
                    .send(vec![
                        self.beat_detector.bass_stats.energy,
                        self.beat_detector.bass_stats.short.avg,
                        self.beat_detector.bass_stats.long.avg,
                        to_float(self.beat_in_tick),
                        self.bpm_tracker.beat_probability(self.samples_processed),
                        self.bpm_tracker.phase_error / 50.0 + 0.5,
                    ])
                    .expect("Failed to broadcast frame bass frequencies");
            }
        }
    }

    // A tick is @ 60Hz
    // A sample is @ 44100Hz
    // A frame is @ 44100Hz / 64 == 689.0625Hz
    pub fn on_tick(&mut self, signal: &RingBuffer<f32>) {
        self.beat_in_tick = false;

        // Run sample-by-sample analysis.
        let (read_index, write_index, buf_size) = self.compute_data_indices(signal);
        if write_index < read_index {
            for index in (read_index..buf_size).chain(0..write_index) {
                self.on_pcm_sample(signal.data[index]);
            }
        } else {
            for index in read_index..write_index {
                self.on_pcm_sample(signal.data[index]);
            }
        }

        // Run DFTs on filtered/split signals.
        let dft_vec = self.signal_dft.get_input_vec();
        // TODO use signal or an analysis copy?
        signal.write_to_buffer(dft_vec);
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
