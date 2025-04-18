use crate::{
    filters::{
        alpha_avg::AlphaAvg,
        biquad_band_pass::BiquadBandPass,
        energy::Energy,
        filter::Filter,
        max_decay_normalizer::MaxDecayNormalizer,
        statistical_summary::{Grade, StatisticalSummary},
    },
    ring_buffer::RingBuffer,
    Args,
};

pub struct BandToFrameEnergy {
    pub filter: BiquadBandPass,
    pub signal_energy: Energy,
    pub frame_energy: f32,
}

impl BandToFrameEnergy {
    fn new(sample_rate: f32, center_fq: usize, q: f32) -> Self {
        Self {
            filter: BiquadBandPass::new(sample_rate, center_fq, q),
            signal_energy: Energy::new(sample_rate as usize / 10),
            frame_energy: 0.0,
        }
    }

    fn sample(&mut self, x: f32) {
        let x = self.filter.sample(x);
        self.frame_energy = self.signal_energy.sample(x);
    }
}

pub struct FrameEnergyStats {
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

impl FrameEnergyStats {
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
    pub bass_energy: BandToFrameEnergy,
    pub bass_stats: FrameEnergyStats,
    pub bass_buffer: RingBuffer<f32>,
}

impl BeatDetector {
    const BEAT_FRAMES_PER_SAMPLE: f32 = 64.0;

    pub fn new(args: &Args, sample_rate: f32) -> Self {
        let audio_buffer_size = (args.audio_buffer_sec * sample_rate) as usize;
        let beat_frames_per_s = sample_rate as f32 / Self::BEAT_FRAMES_PER_SAMPLE;
        Self {
            bass_energy: BandToFrameEnergy::new(sample_rate, 50, 6.0),
            bass_stats: FrameEnergyStats::new(1.0, beat_frames_per_s, args.fastest_bpm as f32),

            bass_buffer: RingBuffer::new(audio_buffer_size),
        }
    }

    pub fn on_pcm_sample(&mut self, sample_index: u64, x: f32) -> bool {
        self.bass_energy.sample(x);
        self.bass_buffer.push(self.bass_energy.frame_energy);

        // Every N pcm samples....
        if sample_index & 0b111111 == 0 {
            self.bass_stats.on_beat_frame(self.bass_energy.frame_energy)
        } else {
            false
        }
    }
}
