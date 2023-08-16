use std::{
    mem,
    ops::{Deref, DerefMut},
    rc::Rc,
};

use crate::{
    dft::Dft,
    error::Error,
    ring_buffer::RingBuffer,
    utils::mix,
    vulkan::{multi_buffer::MultiBuffer, Vulkan},
};

struct History {
    pub values: RingBuffer<f32>,
    pub min: f32,
    pub max: f32,
}

impl Deref for History {
    type Target = RingBuffer<f32>;

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl DerefMut for History {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}

impl History {
    fn new(num_samples: usize) -> Self {
        History {
            values: RingBuffer::new(num_samples),
            min: 0f32,
            max: 0f32,
        }
    }
}

struct AlphaAvg {
    pub alpha: f32,
    pub avg: f32,
}

impl AlphaAvg {
    fn new(alpha: f32) -> Self {
        AlphaAvg { alpha, avg: 0f32 }
    }

    fn sample(&mut self, x: f32) {
        self.avg = mix(self.avg, x, self.alpha);
    }
}

struct WindowedAvg {
    pub size: usize,

    sum: f32,
    pub avg: f32,

    square_sum: f32,
    square_avg: f32,

    pub sd: f32,
}

impl WindowedAvg {
    fn new(size: usize) -> Self {
        WindowedAvg {
            size,
            sum: 0f32,
            avg: 0f32,
            square_sum: 0f32,
            square_avg: 0f32,
            sd: 0f32,
        }
    }

    fn sample(&mut self, old_x: f32, x: f32) {
        self.sum += x - old_x;
        self.avg = self.sum / self.size as f32;

        self.square_sum += x.powf(2f32) - old_x.powf(2f32);
        self.square_avg = self.square_sum / self.size as f32;

        self.sd = (self.square_avg - self.avg.powf(2f32)).sqrt();
    }
}

pub struct BeatAnalysis {
    history: History,
    history_gpu: Rc<MultiBuffer>,

    // Averages.
    long_avg: AlphaAvg,
    short_avg: WindowedAvg,

    // Beat detection.
    noise_threshold_factor: f32,
    beat_sigma_threshold_factor: f32,
    is_high: bool,
    pub is_beat: bool,

    // BPM detection.
    autocorrelation: Dft,
    autocorrelation_gpu: Rc<MultiBuffer>,
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
    pub fn new(vulkan: &mut Vulkan) -> Result<Self, Error> {
        let frame_rate = 60;

        let history_size = 8 * frame_rate;
        let history_gpu_size = 2 * mem::size_of::<i32>() + history_size * mem::size_of::<f32>();

        let autocorrelation_gpu_size = mem::size_of::<i32>() + history_size * mem::size_of::<f32>();

        Ok(Self {
            history: History::new(history_size),
            history_gpu: vulkan.new_multi_buffer("history", history_gpu_size, Some(1))?,
            // Averages.
            long_avg: AlphaAvg::new(0.99),
            short_avg: WindowedAvg::new((0.2 * frame_rate as f32) as usize),
            // Beat detection.
            noise_threshold_factor: 0.25,
            beat_sigma_threshold_factor: 2.2,
            is_high: false,
            is_beat: false,
            // BPM detection.
            autocorrelation: Dft::new(8 * frame_rate),
            autocorrelation_gpu: vulkan.new_multi_buffer(
                "autocorrelation",
                autocorrelation_gpu_size,
                Some(1),
            )?,
        })
    }

    fn update_averages(&mut self, x: f32) {
        self.long_avg.sample(x);

        let old_x = self.history.at_offset(0, self.short_avg.size);
        self.short_avg.sample(*old_x, x);

        self.history.push(x);
    }

    fn decide_beat(&mut self, x: f32) {
        let noise_threshold = self.noise_threshold_factor * self.long_avg.avg;
        let not_noise = self.short_avg.avg > noise_threshold;
        let beat_margin = self.beat_sigma_threshold_factor * self.short_avg.sd;
        let beat_threshold = self.short_avg.avg + beat_margin;
        let loud_outlier = x > beat_threshold;

        let was_high = self.is_high;
        self.is_high = not_noise && loud_outlier;
        self.is_beat = !was_high && self.is_high;
    }

    fn update_bpm(&mut self) {
        // Just the last value.
        self.history.write_to_pointer(
            self.history.offset_index(0, 1),
            self.history.write_index,
            self.history_gpu.mapped(0),
        );

        self.history
            .write_to_buffer(self.autocorrelation.get_input_vec());
        self.autocorrelation.autocorrelate();
        self.autocorrelation
            .write_input_to_pointer(self.autocorrelation_gpu.mapped(0));

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
}
