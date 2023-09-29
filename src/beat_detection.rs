use crate::averages::{AlphaAvg, History, WindowedAvg};

pub struct BeatDetection {
    pub history: History,

    // Averages.
    pub long_avg: AlphaAvg,
    pub short_avg: WindowedAvg,

    // Beat detection.
    pub noise_threshold_factor: f32,
    pub beat_sigma_threshold_factor: f32,
    is_high: bool,
    pub is_beat: bool,
}

fn wrap_index(pos_offset: usize, neg_offset: usize, len: usize) -> usize {
    let idx = pos_offset + len - neg_offset;
    if idx >= len {
        idx % len
    } else {
        idx
    }
}

impl BeatDetection {
    pub fn new() -> Self {
        // TODO make dynamic using on-the-go fps detection and reinitialization.
        let frame_rate = 60;

        let history_size = 8 * frame_rate;
        let short_avg_size = frame_rate / 5;

        Self {
            history: History::new(history_size),
            // Averages.
            long_avg: AlphaAvg::new(0.995),
            short_avg: WindowedAvg::new(short_avg_size),
            // Beat detection.
            noise_threshold_factor: 1.0,
            beat_sigma_threshold_factor: 2.5,
            is_high: false,
            is_beat: false,
        }
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

    fn update_bpm(&mut self) {}

    pub fn sample(&mut self, x: f32) {
        self.update_averages(x);
        self.decide_beat(x);
        self.update_bpm();
    }
}
