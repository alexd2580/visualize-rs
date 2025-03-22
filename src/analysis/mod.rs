pub mod beat_detection;
pub mod dft;
pub mod server;

use std::{
    sync::Arc,
    time::{self, Instant},
};

use dft::Dft;
use server::FrameSender;

use crate::{
    filters::{
        alpha_avg::AlphaAvg, biquad_band_pass::BiquadBandPass, energy::Energy, filter::Filter,
        max_decay_normalizer::MaxDecayNormalizer, statistical_summary::StatisticalSummary,
    },
    ring_buffer::RingBuffer,
    Args,
};

struct BandToFrameEnergy {
    pub filter: BiquadBandPass,
    pub signal_energy: Energy,
    pub frame_energy: f32,
}

impl BandToFrameEnergy {
    fn new(sample_rate: usize, center_fq: usize, q: f32) -> Self {
        Self {
            filter: BiquadBandPass::new(sample_rate, center_fq, q),
            signal_energy: Energy::new(sample_rate / 5),
            frame_energy: 0.0,
        }
    }

    fn sample(&mut self, x: f32) {
        let x = self.filter.sample(x);
        self.frame_energy = self.signal_energy.sample(x);
    }
}

struct FrameEnergyStats {
    pub normalizer: MaxDecayNormalizer,
    pub energy: f32,
    pub short: StatisticalSummary,
    pub long: AlphaAvg,
    pub long_avg_threshold: f32,

    pub under_threshold: bool,
    pub over_threshold: bool,
    pub is_beat: bool,
}

impl FrameEnergyStats {
    fn new() -> Self {
        let frames_per_s = 44100usize / 64;
        Self {
            normalizer: MaxDecayNormalizer::new(0.9999, 0.03),
            energy: 0.0,
            short: StatisticalSummary::new(3 * frames_per_s),
            long: AlphaAvg::new(0.9999),
            long_avg_threshold: 0.4,
            under_threshold: true,
            over_threshold: false,
            is_beat: false,
        }
    }

    fn sample(&mut self, x: f32) {
        self.energy = self.normalizer.sample(x);
        self.short.sample(self.energy);

        let long_avg = self.long.sample(self.energy);
        let longterm_high = self.energy > long_avg.max(self.long_avg_threshold);

        let shortterm_high = self.energy > 1.1 * self.short.avg;
        let shortterm_low = self.energy < 0.9 * self.short.avg;

        let was_over_threshold = self.over_threshold;
        if self.under_threshold && shortterm_high && longterm_high {
            self.under_threshold = false;
            self.over_threshold = true;
        }
        self.is_beat = !was_over_threshold && self.over_threshold;

        if self.over_threshold && shortterm_low {
            self.under_threshold = true;
            self.over_threshold = false;
        }
    }
}

pub struct BpmTracker {
    phase: u64,
    periods: RingBuffer<u64>,
    period: f32,
    bpm: f32,
}

impl BpmTracker {
    fn new() -> Self {
        BpmTracker {
            phase: 0,
            periods: RingBuffer::new_with_default(10, 0),
            period: 60.0,
            bpm: 60.0,
        }
    }

    fn sample(&mut self, phase: u64) {
        let period = phase - self.phase;
        self.periods.push(period);
        self.phase = phase;

        let mut sorted = self.periods.data.clone();
        sorted.sort_unstable();
        let period = (sorted[sorted.len() / 2] + sorted[sorted.len() / 2 + 1]) as f32 / 2.0;
        if period > 0.0 {
            self.period = period;
            self.bpm = 44100.0 * 60.0 / period;
        }
    }

    fn beat_probability(&self, phase: u64) -> f32 {
        let iterations = (phase - self.phase) as f32 / self.period;
        2.0 * (0.5 - iterations.fract()).abs() / iterations.max(1.0)
    }
}

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

    pub signal_dft: Dft,

    bass_energy: BandToFrameEnergy,
    bass_stats: FrameEnergyStats,

    bpm_tracker: BpmTracker,
    beat_in_tick: bool,

    bass_buffer: RingBuffer<f32>,

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
    pub fn new(args: &Args, sample_rate: usize, broadcast: Option<Arc<FrameSender>>) -> Self {
        let audio_buffer_size = args.audio_buffer_sec * sample_rate;

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
            signal_dft: Dft::new(dft_size),

            bass_energy: BandToFrameEnergy::new(sample_rate, 50, 6.0),
            bass_stats: FrameEnergyStats::new(),

            bpm_tracker: BpmTracker::new(),
            beat_in_tick: false,

            bass_buffer: RingBuffer::new(audio_buffer_size),
            // low_pass: LowPass::new(sample_rate, 100),
            // low_pass_buffer: RingBuffer::new(audio_buffer_size),
            // low_pass_dft: Dft::new(args.dft_size),
            // high_pass: HighPass::new(sample_rate, 100),
            // high_pass_buffer: RingBuffer::new(audio_buffer_size),
            // high_pass_dft: Dft::new(args.dft_size),
            // frequency_band_border_indices,
            // beat_dft_range: (beat_dft_lower, beat_dft_upper),
            // beat_detectors: Vec::from_iter(
            //     (beat_dft_lower..=beat_dft_upper).map(|_| BeatDetection::new()),
            // ),
            broadcast,
        }
    }

    /// Compute the read index (start of data to read), write index (index at which new data will
    /// be written (end of data to read) and the size of the ring buffer.
    fn compute_data_indices(&mut self, signal: &RingBuffer<f32>) -> (usize, usize, usize) {
        let read_index = self.bass_buffer.write_index;
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

    fn sample(&mut self, x: f32) {
        let x = self.normalizer.sample(x);
        self.bass_energy.sample(x);
        self.bass_buffer.push(x);

        self.samples_processed += 1;

        // Every 64th sample is a frame.
        if self.samples_processed & 0b111111 == 0 {
            self.bass_stats.sample(self.bass_energy.frame_energy);
            if self.bass_stats.is_beat {
                self.beat_in_tick = true;
                self.bpm_tracker.sample(self.samples_processed);
            }

            let to_float = |x: bool| if x { 1.0 } else { 0.0 };
            if let Some(broadcast) = &self.broadcast {
                broadcast
                    .send(vec![
                        self.bass_stats.energy,
                        self.bass_stats.short.avg,
                        self.bass_stats.long.avg,
                        to_float(self.bass_stats.is_beat),
                        self.bpm_tracker.beat_probability(self.samples_processed),
                    ])
                    .expect("Failed to broadcast frame bass frequencies");
            }
        }

        // self.low_pass_buffer.push(self.low_pass.sample(x));
        // self.high_pass_buffer.push(self.high_pass.sample(x));
    }

    // A tick is @ 60Hz
    // A sample is @ 44100Hz
    // A frame is @ 44100Hz / 64 == 689.0625Hz
    pub fn tick(&mut self, signal: &RingBuffer<f32>) {
        let (read_index, write_index, buf_size) = self.compute_data_indices(signal);

        self.beat_in_tick = false;

        // Run sample-by-sample analysis.
        if write_index < read_index {
            for index in (read_index..buf_size).chain(0..write_index) {
                self.sample(signal.data[index]);
            }
        } else {
            for index in read_index..write_index {
                self.sample(signal.data[index]);
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

        // let beat_dft = &self.low_pass_dft;
        // let bass_frequencies = &beat_dft.simple[self.beat_dft_range.0..=self.beat_dft_range.1];

        // let frequency_sum = bass_frequencies.iter().fold(0.0, |a, b| a + b);

        // let to_float = |x: bool| if x { 1.0 } else { 0.0 };
        // if let Some(broadcast) = &self.broadcast {
        //     broadcast
        //         .send(vec![
        //             self.bass_stats.energy,
        //             self.bass_stats.short.avg,
        //             self.bass_stats.long.avg,
        //             to_float(self.beat_in_tick),
        //         ])
        //         .expect("Failed to broadcast frame bass frequencies");
        // }
    }
}
