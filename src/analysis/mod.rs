pub mod beat_detection;
pub mod dft;
pub mod server;

use std::{
    sync::Arc,
    time::{self, Instant},
};

use dft::Dft;
use log::{info, warn};
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
    fn new(sample_rate: u32, center_fq: usize, q: f32) -> Self {
        Self {
            filter: BiquadBandPass::new(sample_rate, center_fq, q),
            signal_energy: Energy::new(sample_rate as usize / 5),
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
    sample_rate: i64,
    sample_rate_f: f32,

    phase_origin: u64,
    phase: f32,
    error_dt: f32,

    short_period_estimator: AlphaAvg,
    period: AlphaAvg,
    bpm: f32,

    last_beats: RingBuffer<u64>,
    last_delta: RingBuffer<f32>,
}

impl BpmTracker {
    fn new(sample_rate: u32) -> Self {
        let size_history = 30;
        BpmTracker {
            sample_rate: sample_rate as i64,
            sample_rate_f: sample_rate as f32,

            phase_origin: 0,
            phase: 0.0,
            error_dt: 0.0,

            short_period_estimator: AlphaAvg::new_with_value(0.9, 0.5),
            period: AlphaAvg::new_with_value(0.9995, 0.5),
            bpm: 120.0,

            last_beats: RingBuffer::new_with_default(size_history, 0),
            last_delta: RingBuffer::new_with_default(size_history, 0.5),
        }
    }

    fn sample_to_rel_time(&self, sample_index: u64) -> f32 {
        (sample_index - self.phase_origin) as f32 / self.sample_rate_f - self.phase
    }

    fn sample(&mut self, sample_index: u64) {
        /* MOVE THE ORIGIN CLOSER TO THE SERIES TO PREVENT FLOAT IMPRECISION */

        let delta_pre = self
            .sample_to_rel_time(sample_index)
            .rem_euclid(self.period.avg);

        let first_buffered_beat = self.last_beats.data[self.last_beats.write_index];

        // What is the phase relative to this beat? We do this to prevent f32 imprecision and a
        // strong effect of varying BPM relative to a faraway origin (drift? small error has huge
        // impact on the far future). We keep tracking the samples in u64, u32 would wraparound in
        // ~27 hours.
        let delta = (first_buffered_beat - self.phase_origin) as f32 / self.sample_rate_f;
        self.phase_origin = first_buffered_beat;
        // phase - delta gives how much phase (including period) there is at T in the future.
        // The further I go into the future, the more negative the phase (aka origin) becomes.
        // `rem_euclid` ensures the rem result is always positive.
        self.phase = (self.phase - delta).rem_euclid(self.period.avg);

        let delta_post = self
            .sample_to_rel_time(sample_index)
            .rem_euclid(self.period.avg);
        if (delta_pre - delta_post).abs() > 0.00001 {
            warn!("Moving the origin caused a big discrepancy");
        }

        /* FILTER COMPLETELY WRONG BEAT DELTAS */

        // We use these limits to ignore totally off-beat deltas.
        let delta_90 = 60f32 / 90.0; // large
        let delta_180 = 60f32 / 180.0; // small

        // Compute delta to previous beat.
        let delta_s = (sample_index - self.last_beats.last()) as f32 / self.sample_rate_f;
        // Always record the last beat!
        self.last_beats.push(sample_index);
        // Ignore the delta if it doesn't fit our expectations.
        if delta_180 < delta_s && delta_s < delta_90 {
            self.last_delta.push(delta_s);
        }

        /* RECOMPUTE BPM USING MEDIAN + SLOWLY MOVING AVERAGE */

        let mut sorted = self.last_delta.data.clone();
        sorted.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let estimated_period = (sorted[sorted.len() / 2] + sorted[sorted.len() / 2 + 1]) / 2.0;

        self.short_period_estimator.sample(estimated_period);
        self.period.sample(estimated_period);

        /* ALLOW FOR QUICK TEMPO CHANGE USING SHORT TERM OVERRIDE */

        // Diff greater than 2 BPM.
        if (self.short_period_estimator.avg - self.period.avg).abs() > 0.01 {
            info!(
                "Shortterm delta {:.1} deviates from longterm delta {:.1}. Resetting long-term avg",
                60.0 / self.short_period_estimator.avg,
                60.0 / self.period.avg
            );
            self.period.avg = self.short_period_estimator.avg;
        }
        // self.period.avg = 60.0 / 147.0;
        self.bpm = 60.0 / self.period.avg;

        /* FIT THE PHASE ONTO THE LAST KNOWN BEATS USING GRADIENT DESCENT */

        let error_with_phase = |phase: f32| {
            self.last_beats
                .data
                .iter()
                .map(|index| {
                    let time = (index - self.phase_origin) as f32 / self.sample_rate_f;
                    let offset = (time - phase) / self.period.avg;
                    (2.0 * (offset - offset.round())).powi(2)
                })
                .sum::<f32>()
        };

        let delta_t = 0.001 * estimated_period; // 0.1% of period
        let error_now = error_with_phase(self.phase);
        let error_offset = error_with_phase(self.phase + delta_t);
        self.error_dt = (error_offset - error_now) / delta_t;

        // Bigger phase shifts beats forward.
        // Bigger period shifts beats forward.
        self.phase -= 0.0005 * self.error_dt;
        // self.period.avg += 0.00003 * self.error_dt;
        // dbg!((self.error_dt, self.phase));

        // let error_dt = error_delta / delta_t;
        // if error_delta.abs() > 0.001 {
        //     let step = delta_t * error_now / (error_offset - error_now);
        //     // if step < 0.1 * estimated_period {
        //     self.phase = self.phase - step;
        //
        //     let new_error = error_with_phase(self.phase);
        //     info!("{:.3} -> {:.3}", error_now, new_error);
        //
        //     // } else {
        //     //     warn!("En {:.3}; En+1 {:.3}", error_now, error_offset);
        //     //     warn!("Got extreme newton step {:.3}ms. Skipping.", step * 1000.0);
        //     // }
        //     dbg!(self.phase);
        // }
    }

    fn beat_probability(&self, sample_index: u64) -> f32 {
        let offset = (sample_index - self.phase_origin) as f32 / self.sample_rate_f - self.phase;
        let offset = (offset / self.period.avg).fract();
        2.0 * (offset - 0.5).abs()
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
    pub fn new(args: &Args, sample_rate: u32, broadcast: Option<Arc<FrameSender>>) -> Self {
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
            signal_dft: Dft::new(dft_size),

            bass_energy: BandToFrameEnergy::new(sample_rate, 50, 6.0),
            bass_stats: FrameEnergyStats::new(),

            bpm_tracker: BpmTracker::new(sample_rate),
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
                        // self.bpm_tracker.error_dt / 400.0 + 0.5,
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
