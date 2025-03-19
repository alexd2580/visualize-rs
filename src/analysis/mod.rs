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
    audio::Audio,
    filters::{
        biquad_band_pass::BiquadBandPass, energy::Energy, filter::Filter,
        max_decay_normalizer::MaxDecayNormalizer, statistical_summary::StatisticalSummary,
    },
    ring_buffer::RingBuffer,
    Args,
};

struct BeatDetector {
    pub filter: BiquadBandPass,
    pub signal_energy: Energy,
    pub energy_normalizer: MaxDecayNormalizer,
    pub energy_stats: StatisticalSummary,

    pub frame_energy: f32,
    pub is_beat: bool,
}

const FRAME_RATE: usize = 60;

impl BeatDetector {
    fn new(sample_rate: usize, center_fq: usize) -> Self {
        Self {
            filter: BiquadBandPass::new(sample_rate, center_fq, 10.0),
            signal_energy: Energy::new(sample_rate / 5),
            energy_normalizer: MaxDecayNormalizer::new(0.995, 0.1),
            energy_stats: StatisticalSummary::new(FRAME_RATE * 3),
            frame_energy: 0.0,
            is_beat: false,
        }
    }

    fn sample(&mut self, x: f32) {
        let x = self.filter.sample(x);
        self.signal_energy.sample(x);
    }

    fn frame(&mut self) {
        self.frame_energy = self.energy_normalizer.sample(self.signal_energy.energy());
        self.energy_stats.sample(self.frame_energy);
        self.is_beat = self.frame_energy > self.energy_stats.avg
    }
}

/// Note the reverse drop order.
pub struct Analysis {
    pub epoch: time::Instant,

    pub audio: Audio,

    pub read_index: usize,
    pub write_index: usize,
    pub buf_size: usize,

    available_samples: usize,
    avg_available_samples: f32,
    avg_available_samples_alpha: f32,

    normalizer: MaxDecayNormalizer,

    pub signal_dft: Dft,

    bass_70: BeatDetector,
    bass_90: BeatDetector,
    bass_110: BeatDetector,

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
    pub fn new(args: &Args, audio: Audio, broadcast: Option<Arc<FrameSender>>) -> Self {
        let audio_buffer_size = audio.buffer_size();

        let dft_size = args.dft_size;
        let sample_rate = audio.sample_rate();

        let dft_window_per_s = sample_rate as f32 / dft_size as f32;
        let dft_min_fq = dft_window_per_s * 1f32;
        let dft_max_fq = dft_window_per_s * dft_size as f32 / 2f32;
        log::info!("DFT can analyze frequencies in the range: {dft_min_fq} hz - {dft_max_fq} hz");

        // let frequency_band_borders = [16, 60, 250, 500, 2000, 4000, 6000, 22000];
        // let frequency_band_border_indices = frequency_band_borders
        //     .map(|frequency| dft_index_of_frequency(frequency, audio.sample_rate(), dft_size));
        //
        // let beat_dft_lower = dft_index_of_frequency(35, audio.sample_rate(), dft_size);
        // let beat_dft_upper = dft_index_of_frequency(125, audio.sample_rate(), dft_size);

        Self {
            epoch: Instant::now(),
            audio,
            read_index: 0,
            write_index: 0,
            buf_size: 0,
            available_samples: 0,
            avg_available_samples: 44100f32 / 60f32,
            avg_available_samples_alpha: 0.95,
            normalizer: MaxDecayNormalizer::new(0.99995, 0.05),
            signal_dft: Dft::new(args.dft_size),
            bass_70: BeatDetector::new(sample_rate, 70),
            bass_90: BeatDetector::new(sample_rate, 90),
            bass_110: BeatDetector::new(sample_rate, 110),
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
    fn compute_data_indices(&mut self) -> (usize, usize, usize) {
        let read_index = self.bass_buffer.write_index;
        let write_index = self.audio.signal.write_index;
        let buf_size = self.audio.signal.data.len();

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

        // `+5` makes it so that i try to display more frames without lagging behind too much.
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

        self.bass_70.sample(x);
        self.bass_90.sample(x);
        self.bass_110.sample(x);

        self.bass_buffer.push(x);

        // self.low_pass_buffer.push(self.low_pass.sample(x));
        // self.high_pass_buffer.push(self.high_pass.sample(x));
    }

    pub fn tick(&mut self) {
        let (read_index, write_index, buf_size) = self.compute_data_indices();

        // Run sample-by-sample analysis.
        if write_index < read_index {
            for index in (read_index..buf_size).chain(0..write_index) {
                self.sample(self.audio.signal.data[index]);
            }
        } else {
            for index in read_index..write_index {
                self.sample(self.audio.signal.data[index]);
            }
        }

        // Run DFTs on filtered/split signals.
        let dft_vec = self.signal_dft.get_input_vec();
        self.audio.signal.write_to_buffer(dft_vec);
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

        self.bass_70.frame();
        self.bass_90.frame();
        self.bass_110.frame();

        let to_float = |x: bool| if x { 1.0 } else { 0.0 };

        if let Some(broadcast) = &self.broadcast {
            broadcast
                .send(vec![
                    self.bass_70.frame_energy,
                    self.bass_70.energy_stats.avg,
                    self.bass_70.energy_stats.sd,
                    to_float(self.bass_70.is_beat),
                    self.bass_90.frame_energy,
                    self.bass_90.energy_stats.avg,
                    self.bass_90.energy_stats.sd,
                    to_float(self.bass_90.is_beat),
                    self.bass_110.frame_energy,
                    self.bass_110.energy_stats.avg,
                    self.bass_110.energy_stats.sd,
                    to_float(self.bass_110.is_beat),
                ])
                .expect("Failed to broadcast frame bass frequencies");
        }
    }
}
