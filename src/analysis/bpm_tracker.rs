use tracing::{debug, warn};

use crate::{ring_buffer::RingBuffer, Args};

pub struct Mode {
    min_value: u32,
    values: RingBuffer<u32>,
    counter: Vec<u32>,
}

impl Mode {
    fn new(min_value: u32, max_value: u32, size: usize) -> Self {
        Mode {
            min_value,
            values: RingBuffer::new_with_default(size, min_value),
            counter: vec![0; (max_value - min_value) as usize],
        }
    }

    fn sample(&mut self, next: u32) -> u32 {
        let last = self.values.data[self.values.write_index];
        self.values.push(next);

        let last_index = (last - self.min_value) as usize;
        self.counter[last_index] = self.counter[last_index].saturating_sub(1);
        let next_index = (next - self.min_value) as usize;
        self.counter[next_index] += 1;

        let most_frequent = self
            .counter
            .iter()
            .enumerate()
            .max_by_key(|(_index, count)| *count)
            .unwrap();

        most_frequent.0 as u32 + self.min_value
    }
}

pub struct BpmTracker {
    // Constant.
    sample_rate: f32,
    delta_slow: f32,
    delta_fast: f32,

    // Universal.
    beat_index: u32,
    last_beats: RingBuffer<u64>,
    last_delta: RingBuffer<f32>,

    // Period.
    last_delta_sum: f32,
    bpm_mode: Mode,
    bpm_candidate: u32,
    bpm: u32,

    // Phase.
    phase_origin: u64,
    phase: f32,

    pub phase_error: f32,
    pub phase_error_dt: f32,
}

impl BpmTracker {
    const BEATS_HISTORY_SIZE: usize = 15;
    const DELTA_HISTORY_SIZE: usize = 10;
    const BPM_HISTORY_SIZE: usize = 32;

    pub fn new(args: &Args, sample_rate: f32) -> Self {
        let rough_bpm = (args.slowest_bpm + args.fastest_bpm) / 2;
        BpmTracker {
            // Constant.
            sample_rate,
            // We use these limits to ignore totally off-beat deltas.
            delta_slow: 60.0 / args.slowest_bpm as f32,
            delta_fast: 60.0 / args.fastest_bpm as f32,

            // Universal.
            beat_index: 0,
            last_beats: RingBuffer::new_with_default(Self::BEATS_HISTORY_SIZE, 0),
            last_delta: RingBuffer::new_with_default(Self::DELTA_HISTORY_SIZE, 0.5),

            // Period.
            last_delta_sum: 0.5 * Self::DELTA_HISTORY_SIZE as f32,
            bpm_mode: Mode::new(args.slowest_bpm, args.fastest_bpm, Self::BPM_HISTORY_SIZE),
            bpm_candidate: rough_bpm,
            bpm: rough_bpm,

            // Phase.
            phase_origin: 0,
            phase: 0.0,

            phase_error: 0.0,
            phase_error_dt: 0.0,
        }
    }

    fn delta_fits_bpm_range(&self, delta_s: f32) -> bool {
        self.delta_fast < delta_s && delta_s < self.delta_slow
    }

    fn sample_to_rel_time(&self, sample_index: u64) -> f32 {
        (sample_index - self.phase_origin) as f32 / self.sample_rate - self.phase
    }

    /// Filter completely wrong beat deltas.
    fn record_delta(&mut self, sample_index: u64) {
        // Compute delta to previous beat.
        let delta_s = (sample_index - self.last_beats.last()) as f32 / self.sample_rate;

        // Always record the last beat!
        self.last_beats.push(sample_index);
        // Ignore the delta if it doesn't fit our expectations.
        if self.delta_fits_bpm_range(delta_s) {
            // let bpm_f = 60.0 * (Self::DELTA_HISTORY_SIZE as f32) / self.last_delta_sum;
            // let this_bpm = 60.0 / delta_s;
            // let filtered_bpm = this_bpm
            //     .max(bpm_f - Self::BPM_FILTER_SIDEBAND)
            //     .min(bpm_f + Self::BPM_FILTER_SIDEBAND);
            // let delta_s = 60.0 / filtered_bpm;

            self.last_delta_sum += delta_s - self.last_delta.data[self.last_delta.write_index];
            self.last_delta.push(delta_s);
        }

        // match self.beat_track_stage {
        //     BeatTrackStage::Bpm => self.detect_bpm(),
        //     BeatTrackStage::RoughPhase => self.rough_phase(),
        //     BeatTrackStage::FinePhase => self.fine_phase(),
        // }
    }

    fn estimate_bpm(&mut self) {
        let bpm = (60.0 * (Self::DELTA_HISTORY_SIZE as f32) / self.last_delta_sum).round() as u32;
        self.bpm_candidate = self.bpm_mode.sample(bpm);
    }

    fn check_bpm_candidate(&mut self) {
        if self.bpm_candidate == self.bpm {
            return;
        }

        let current_period = 60.0 / self.bpm as f32;
        let error_now = self.error_phase_period(self.phase, current_period);

        let candidate_period = 60.0 / self.bpm_candidate as f32;
        let best_candidate_phase = (0..10)
            .map(|step| {
                let candidate_phase = step as f32 * candidate_period / 10.0;
                return (
                    candidate_phase,
                    self.error_phase_period(candidate_phase, candidate_period),
                );
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap();

        if best_candidate_phase.1 < error_now {
            self.bpm = self.bpm_candidate;
            self.phase = best_candidate_phase.0;
            debug!("Switched BPM to {}", self.bpm);
        } else {
            debug!("Denied BPM switch to {}", self.bpm_candidate);
        }
    }

    fn error_phase_period(&self, phase: f32, period: f32) -> f32 {
        self.last_beats
            .data
            .iter()
            .map(|index| {
                let time = (index - self.phase_origin) as f32 / self.sample_rate;
                let offset = (time - phase) / period;
                (2.0 * (offset - offset.round())).powi(2)
            })
            .sum::<f32>()
    }

    pub fn on_beat(&mut self, sample_index: u64) {
        self.beat_index += 1;
        /* MOVE THE ORIGIN CLOSER TO THE SERIES TO PREVENT FLOAT IMPRECISION */

        let period = 60.0 / self.bpm as f32;

        let delta_pre = self.sample_to_rel_time(sample_index).rem_euclid(period);

        let first_buffered_beat = self.last_beats.data[self.last_beats.write_index];

        // What is the phase relative to this beat? We do this to prevent f32 imprecision and a
        // strong effect of varying BPM relative to a faraway origin (drift? small error has huge
        // impact on the far future). We keep tracking the samples in u64, u32 would wraparound in
        // ~27 hours.
        let delta = (first_buffered_beat - self.phase_origin) as f32 / self.sample_rate;
        self.phase_origin = first_buffered_beat;
        // phase - delta gives how much phase (including period) there is at T in the future.
        // The further I go into the future, the more negative the phase (aka origin) becomes.
        // `rem_euclid` ensures the rem result is always positive.
        self.phase = (self.phase - delta).rem_euclid(period);

        let delta_post = self.sample_to_rel_time(sample_index).rem_euclid(period);
        if (delta_pre - delta_post).abs() > 0.00001 {
            warn!("Moving the origin caused a big discrepancy");
        }

        self.record_delta(sample_index);
        self.estimate_bpm();

        // Check the BPM candidate every fourth beat.
        if self.beat_index & 0b11 == 0 {
            self.check_bpm_candidate();
        }

        /* FIT THE PHASE ONTO THE LAST KNOWN BEATS USING GRADIENT DESCENT */

        let period = 60.0 / self.bpm as f32;
        let delta_t = 0.001 * period; // 0.1% of period
        let error_now = self.error_phase_period(self.phase, period);
        let error_offset = self.error_phase_period(self.phase + delta_t, period);
        let error_dt = (error_offset - error_now) / delta_t;

        // Bigger phase shifts beats forward.
        // Bigger period shifts beats forward.
        let five_percent = 0.05 * period;
        self.phase -= (0.0005 * error_dt).clamp(-five_percent, five_percent);

        self.phase_error = error_now;
        self.phase_error_dt = error_dt;
    }

    pub fn beat_probability(&self, sample_index: u64) -> f32 {
        let error = (0.5 / self.phase_error).min(1.0);

        let offset = (sample_index - self.phase_origin) as f32 / self.sample_rate - self.phase;
        let period = 60.0 / self.bpm as f32;
        let offset = (offset / period).fract();

        error * 2.0 * (offset - 0.5).abs()
    }
}
