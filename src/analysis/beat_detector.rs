use crate::{
    filters::{
        alpha_avg::AlphaAvg,
        biquad_band_pass::BiquadBandPass,
        energy::Energy,
        filter::Filter,
        max_decay_normalizer::MaxDecayNormalizer,
        statistical_summary::{Grade, StatisticalSummary},
    },
    Args,
};

pub struct BeatStats {
    pub frames_since_last_beat: u32,
    pub min_frames_threshold: u32,

    pub normalizer: MaxDecayNormalizer,
    pub energy: f32,
    pub short: StatisticalSummary,
    pub long: AlphaAvg,
    pub long_avg_threshold: f32,

    pub under_threshold: bool,
    pub over_threshold: bool,

    pub is_beat: bool,
}

impl BeatStats {
    fn new(short_stat_s: f32, beat_checking_frequency: f32, fastest_registered_bpm: f32) -> Self {
        let min_frames_threshold = beat_checking_frequency * 60.0 / fastest_registered_bpm;
        let short_stat_frames = beat_checking_frequency * short_stat_s;

        Self {
            frames_since_last_beat: 0,
            min_frames_threshold: min_frames_threshold.round() as u32,
            normalizer: MaxDecayNormalizer::new(0.9999, 0.03),
            energy: 0.0,
            short: StatisticalSummary::new(short_stat_frames as usize),
            long: AlphaAvg::new(0.9999),
            long_avg_threshold: 0.4,
            under_threshold: true,
            over_threshold: false,
            is_beat: false,
        }
    }

    fn on_beat_frame(&mut self, x: f32) -> bool {
        self.energy = self.normalizer.sample(x);

        self.short.sample(self.energy);
        let shortterm_grade = self.short.grade(self.energy);

        let long_avg = self.long.sample(self.energy);
        let longterm_high = self.energy > long_avg.max(self.long_avg_threshold);

        let was_over_threshold = self.over_threshold;

        if self.under_threshold && shortterm_grade == Grade::High && longterm_high {
            self.under_threshold = false;
            self.over_threshold = true;
        }

        let sub_bpm_limit = self.frames_since_last_beat > self.min_frames_threshold;
        self.is_beat = !was_over_threshold && self.over_threshold && sub_bpm_limit;
        if self.is_beat {
            self.frames_since_last_beat = 0;
        }

        if self.over_threshold && shortterm_grade == Grade::Low {
            self.under_threshold = true;
            self.over_threshold = false;
        }

        self.frames_since_last_beat += 1;
        self.is_beat
    }
}

pub struct BeatDetector {
    pub filter: BiquadBandPass,
    pub energy: Energy,
    pub stats: BeatStats,
}

impl BeatDetector {
    const BEAT_FRAMES_PER_SAMPLE: f32 = 64.0;

    pub fn new(args: &Args, sample_rate: f32) -> Self {
        let beat_frames_per_s = sample_rate / Self::BEAT_FRAMES_PER_SAMPLE;
        Self {
            filter: BiquadBandPass::new(sample_rate, 50, 6.0),
            energy: Energy::new(sample_rate as usize / 10),
            stats: BeatStats::new(1.0, beat_frames_per_s, args.fastest_bpm as f32),
        }
    }

    pub fn on_pcm_sample(&mut self, sample_index: u64, x: f32) -> (f32, bool) {
        let filtered = self.filter.sample(x);
        let energy = self.energy.sample(filtered);

        // Every N pcm samples....
        let is_beat = if sample_index & 0b111111 == 0 {
            self.stats.on_beat_frame(energy)
        } else {
            false
        };

        (energy, is_beat)
    }
}
