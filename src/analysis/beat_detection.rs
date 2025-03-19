use crate::filters::{
    alpha_avg::AlphaAvg, filter::Filter, statistical_summary::StatisticalSummary,
};

pub struct BeatDetection {
    // Averages.
    pub long_avg: AlphaAvg,
    pub short_stats: StatisticalSummary,

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

        let short_stat_size = frame_rate / 5;

        Self {
            // Averages.
            long_avg: AlphaAvg::new(0.995),
            short_stats: StatisticalSummary::new(short_stat_size),
            // Beat detection.
            noise_threshold_factor: 1.0,
            beat_sigma_threshold_factor: 2.5,
            is_high: false,
            is_beat: false,
        }
    }

    fn update_averages(&mut self, x: f32) {
        self.long_avg.sample(x);
        self.short_stats.sample(x);
    }

    fn decide_beat(&mut self, x: f32) {
        let noise_threshold = self.noise_threshold_factor * self.long_avg.avg;
        let not_noise = self.short_stats.avg > noise_threshold;
        let beat_margin = self.beat_sigma_threshold_factor * self.short_stats.sd;
        let beat_threshold = self.short_stats.avg + beat_margin;
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
