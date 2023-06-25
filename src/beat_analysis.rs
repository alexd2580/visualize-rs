use std::{
    collections::{HashMap, VecDeque},
    time,
};

use log::debug;

use crate::{ring_buffer::RingBuffer, vulkan::Vulkan};

#[derive(Eq, Hash, PartialEq, Clone, Debug)]
pub struct BeatStream {
    pub period: time::Duration,
    pub count: usize,
}

pub struct BeatAnalysis {
    // Averages.
    last_values: RingBuffer<f32>,

    // TODO maybe a floating average is better for the long_sum.
    long_avg_size: usize,
    long_sum: f32,
    // Cached value.
    long_avg: f32,

    short_avg_size: usize,
    short_sum: f32,
    // Cached value.
    short_avg: f32,
    square_sum: f32,
    square_avg: f32,

    standard_deviation: f32,

    // Beat detection.
    noise_threshold_factor: f32,
    beat_sigma_threshold_factor: f32,
    is_high: bool,
    pub is_beat: bool,

    // BPM detection.
    last_beats: VecDeque<time::Instant>,
    beat_streams: HashMap<time::Instant, Vec<BeatStream>>,

    // Deprecated
    // Whether there is currentle an extraordinary signal energy.
    pub beat_count: usize,
    pub matches_bpm: bool,
    pub last_bpm_beat: time::Instant,
    pub next_bpm_beat: time::Instant,
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
    pub fn new(vulkan: &Vulkan, short_avg_size: usize, long_avg_size: usize) -> Self {
        let last_values = RingBuffer::new(long_avg_size);
        Self {
            // History.
            last_values,
            // Averages.
            long_avg_size,
            long_sum: 0f32,
            long_avg: 0f32,
            short_avg_size,
            short_sum: 0f32,
            short_avg: 0f32,
            square_sum: 0f32,
            square_avg: 0f32,
            standard_deviation: 0f32,
            // Beat detection.
            noise_threshold_factor: 0.25,
            beat_sigma_threshold_factor: 2.2,
            is_high: false,
            is_beat: false,
            // BPM detection.
            last_beats: VecDeque::new(),
            beat_streams: HashMap::new(),
            //Deprecated
            beat_count: 0,
            matches_bpm: false,
            last_bpm_beat: time::Instant::now(),
            next_bpm_beat: time::Instant::now() + time::Duration::from_secs(1),
        }
    }

    fn update_averages(&mut self, x: f32) {
        // Update averages.
        let long_sum_read_value = self.last_values.at_offset(0, self.long_avg_size);
        self.long_sum = self.long_sum - long_sum_read_value + x;
        self.long_avg = self.long_sum / self.long_avg_size as f32;

        let short_sum_read_value = self.last_values.at_offset(0, self.short_avg_size);
        self.short_sum = self.short_sum - short_sum_read_value + x;
        self.short_avg = self.short_sum / self.short_avg_size as f32;

        let square_sum_read_value = short_sum_read_value.powf(2f32);
        self.square_sum = self.square_sum - square_sum_read_value + x.powf(2f32);
        self.square_avg = self.square_sum / self.short_avg_size as f32;

        self.standard_deviation = (self.square_avg - self.short_avg.powf(2f32)).sqrt();

        // Update history.
        self.last_values.push(x);
    }

    fn decide_beat(&mut self, x: f32) {
        let noise_threshold = self.noise_threshold_factor * self.long_avg;
        let not_noise = self.short_avg > noise_threshold;
        let beat_margin = self.beat_sigma_threshold_factor * self.standard_deviation;
        let beat_threshold = self.short_avg + beat_margin;
        let loud_outlier = x > beat_threshold;

        let was_high = self.is_high;
        self.is_high = not_noise && loud_outlier;
        self.is_beat = !was_high && self.is_high;
    }

    fn update_bpm(&mut self) {



        // if !self.is_beat {
        //     return;
        // }
        //
        // // We know it's a beat (just not for which rhythm...).
        // let timestamp = time::Instant::now();
        //
        // // Drop all previous cached beat timestamps which are not relevant anymore.
        // // These would be slower than X BPM.
        // let drop_offset = time::Duration::from_secs(5); // 12 BPM
        // while self
        //     .last_beats
        //     .front()
        //     .is_some_and(|prev_beat_time| timestamp > *prev_beat_time + drop_offset)
        // {
        //     self.last_beats.pop_front();
        // }
        //
        // // Drop all beat specs that are old and probably dead anyway.
        // // No activity within N seconds.
        // self.beat_streams
        //     .retain(|last, _| *last > timestamp - drop_offset);
        //
        // // Try to match the beat with all beat streams,
        // // each of which has a specific tempo and phase.
        // let mut changes = Vec::new();
        // for (last, specs) in self.beat_streams.iter_mut() {
        //     // Where woule the closest beat of this beat stream be to `timestamp`?
        //     let relative = (timestamp - *last).as_secs_f32();
        //
        //     specs.retain(|spec| {
        //         let period = spec.period.as_secs_f32();
        //         let iterations = (relative / period).round();
        //         if iterations < 0.5 {
        //             // i.e. We haven't completed a full cycle yet.
        //             return true;
        //         }
        //         let new_period = relative / iterations;
        //
        //         // If the offset "off" the expected beat is smaller than 5% of the period
        //         // (maybe absolute?) then this beat fits the spec.
        //         let delta = relative_delta(period, new_period);
        //         debug!(
        //             "{} -> {:.3} {:.3} => {:.3}",
        //             spec.count, period, new_period, delta
        //         );
        //         if delta < 0.1 {
        //             changes.push((new_period, spec.count + 1));
        //             return false;
        //         }
        //
        //         return true;
        //     });
        // }
        //
        // changes.into_iter().for_each(|(period, )|)
        //
        // // Try to start new beat streams with any beat from `last_beats`.
        // for other in self.last_beats.iter() {
        //     let period = timestamp - *other;
        //     if self
        //         .beat_streams
        //         .iter()
        //         .find(|stream| {
        //             stream.last == timestamp
        //                 && relative_delta(stream.period.as_secs_f32(), period.as_secs_f32()) < 0.05
        //         })
        //         .is_none()
        //     {
        //         self.beat_streams.push(BeatStream {
        //             last: timestamp,
        //             period,
        //             count: 2,
        //         });
        //     }
        // }
        //
        // // Push the current beat into last_beats so that new beat streams can start here.
        // self.last_beats.push_back(timestamp);
        //
        // // debug!("{:#?}", self.beat_streams);
        // debug!(
        //     "{} {}",
        //     self.beat_streams.len(),
        //     self.beat_streams
        //         .iter()
        //         .map(|x| x.count)
        //         .fold(0, |a, b| a.max(b))
        // );
        // println!("");
    }

    pub fn sample(&mut self, x: f32) {
        self.update_averages(x);
        self.decide_beat(x);
        self.update_bpm();
    }

    pub fn last_beat(&self) -> time::Instant {
        return time::Instant::now() - time::Duration::from_secs(5000);
        // self.beat_timestamps[wrap_index(self.beat_count, 1, self.beat_timestamps.len())]
    }

    pub fn next_beat(&self) -> time::Instant {
        return time::Instant::now() + time::Duration::from_secs(5000);
        // self.last_beat() + time::Duration::from_secs_f32(self.spb)
    }
}
