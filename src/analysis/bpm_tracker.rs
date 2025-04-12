use tracing::debug;

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
        let last = self.values.oldest();
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

#[derive(Clone)]
pub struct Bpm {
    pub value: u32,
    pub period: f32,
}

impl Bpm {
    fn new(bpm: u32) -> Self {
        Self {
            value: bpm,
            period: 60.0 / bpm as f32,
        }
    }
}

pub struct BpmTracker {
    // Constant.
    sample_rate: f32,
    slow: Bpm,
    fast: Bpm,

    // Universal.
    beat_index: u32,
    last_beats: RingBuffer<u64>,
    on_phase_beats: RingBuffer<u64>,
    last_delta: RingBuffer<f32>,

    // Period.
    last_delta_sum: f32,
    bpm_mode: Mode,

    bpm_candidate: Bpm,
    pub bpm: Bpm,

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
            slow: Bpm::new(args.slowest_bpm),
            fast: Bpm::new(args.fastest_bpm),

            // Universal.
            beat_index: 0,
            last_beats: RingBuffer::new_with_default(Self::BEATS_HISTORY_SIZE, 0),
            on_phase_beats: RingBuffer::new_with_default(Self::BEATS_HISTORY_SIZE, 0),
            last_delta: RingBuffer::new_with_default(Self::DELTA_HISTORY_SIZE, 0.5),

            // Period.
            last_delta_sum: 0.5 * Self::DELTA_HISTORY_SIZE as f32,
            bpm_mode: Mode::new(args.slowest_bpm, args.fastest_bpm, Self::BPM_HISTORY_SIZE),

            bpm_candidate: Bpm::new(rough_bpm),
            bpm: Bpm::new(rough_bpm),

            // Phase.
            phase_origin: 0,
            phase: 0.0,

            phase_error: 0.0,
            phase_error_dt: 0.0,
        }
    }

    fn delta_fits_bpm_range(&self, delta_s: f32) -> bool {
        self.fast.period < delta_s && delta_s < self.slow.period
    }

    fn sample_to_phase(&self, sample_index: u64) -> f32 {
        (sample_index - self.phase_origin) as f32 / self.sample_rate - self.phase
    }

    pub fn sample_to_beat_fract(&self, sample_index: u64) -> f32 {
        (self.sample_to_phase(sample_index) / self.bpm.period).fract()
    }

    /// Filter completely wrong beat deltas.
    fn record_delta(&mut self, sample_index: u64) {
        // Compute delta to previous beat.
        let delta_s = (sample_index - self.last_beats.prev()) as f32 / self.sample_rate;

        // Always record the last beat!
        self.last_beats.push(sample_index);
        // Ignore the delta if it doesn't fit our expectations.
        if self.delta_fits_bpm_range(delta_s) {
            self.last_delta_sum += delta_s - self.last_delta.oldest();
            self.last_delta.push(delta_s);
        }
    }

    fn estimate_bpm(&mut self) {
        let bpm = (60.0 * (Self::DELTA_HISTORY_SIZE as f32) / self.last_delta_sum).round() as u32;
        self.bpm_candidate = Bpm::new(self.bpm_mode.sample(bpm));
    }

    fn check_bpm_candidate(&mut self) {
        if self.bpm_candidate.value == self.bpm.value {
            return;
        }

        let error_now = self.error_phase_period(self.phase, self.bpm.period);

        let best_candidate_phase = (0..10)
            .map(|step| {
                let candidate_phase = step as f32 * self.bpm_candidate.period / 10.0;
                return (
                    candidate_phase,
                    self.error_phase_period(candidate_phase, self.bpm_candidate.period),
                );
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap();

        if best_candidate_phase.1 < error_now {
            self.bpm = self.bpm_candidate.clone();
            self.phase = best_candidate_phase.0;
            debug!("Switched BPM to {}", self.bpm.value);
        } else {
            debug!("Denied BPM switch to {}", self.bpm_candidate.value);
        }
    }

    fn error_phase_period(&self, phase: f32, period: f32) -> f32 {
        self.on_phase_beats
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
        let oldest_beat = self.on_phase_beats.oldest();
        let delta_s = (oldest_beat - self.phase_origin) as f32 / self.sample_rate;
        let num_periods = (delta_s / self.bpm.period).floor();
        self.phase_origin += (num_periods * self.bpm.period) as u64;

        // debug!(
        //     "{oldest_beat} {} {}",
        //     self.phase_origin,
        //     self.phase_origin < oldest_beat
        // );

        // Compute delta to previous beat.
        let delta_s = (sample_index - self.last_beats.prev()) as f32 / self.sample_rate;

        // Always record the last beat!
        self.last_beats.push(sample_index);
        // Ignore the delta if it doesn't fit our expectations.
        if self.delta_fits_bpm_range(delta_s) {
            self.on_phase_beats.push(sample_index);

            self.last_delta_sum += delta_s - self.last_delta.oldest();
            self.last_delta.push(delta_s);
        }
        self.estimate_bpm();

        // Check the BPM candidate every fourth beat.
        if self.beat_index & 0b11 == 0 {
            self.check_bpm_candidate();
        }

        /* FIT THE PHASE ONTO THE LAST KNOWN BEATS USING GRADIENT DESCENT */

        let delta_t = 0.001 * self.bpm.period; // 0.1% of period
        let error_now = self.error_phase_period(self.phase, self.bpm.period);
        let error_offset = self.error_phase_period(self.phase + delta_t, self.bpm.period);
        let error_dt = (error_offset - error_now) / delta_t;

        let five_percent = 0.05 * self.bpm.period;
        self.phase -= (0.0005 * error_dt).clamp(-five_percent, five_percent);

        self.phase_error = error_now;
        self.phase_error_dt = error_dt;
    }

    pub fn bpm_confidence(&self) -> f32 {
        (0.5 / self.phase_error).min(1.0)
    }

    pub fn beat_probability(&self, sample_index: u64) -> f32 {
        let offset = self.sample_to_beat_fract(sample_index);
        self.bpm_confidence() * 2.0 * (offset - 0.5).abs()
    }
}
