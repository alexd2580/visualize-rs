use std::{
    sync::Arc,
    time::{self, Instant},
};

use crate::{
    audio::{high_pass::HighPass, low_pass::LowPass, Audio},
    beat_analysis::BeatAnalysis,
    dft::Dft,
    server::FrameSender,
    utils::dft_index_of_frequency,
    Args,
};

/// Note the reverse drop order.
pub struct Analysis {
    pub epoch: time::Instant,

    pub audio: Audio,

    available_samples: usize,
    avg_available_samples: f32,
    avg_available_samples_alpha: f32,

    pub signal_dft: Dft,

    pub low_pass: LowPass,
    pub low_pass_dft: Dft,

    pub high_pass: HighPass,
    pub high_pass_dft: Dft,

    pub read_index: usize,
    pub write_index: usize,
    pub buf_size: usize,

    pub frequency_band_border_indices: [usize; 8],
    pub beat_analysis: BeatAnalysis,

    broadcast: Option<Arc<FrameSender>>,
}

impl Analysis {
    pub fn new(args: &Args, audio: Audio, broadcast: Option<Arc<FrameSender>>) -> Self {
        let audio_buffer_size = audio.buffer_size();

        let dft_size = args.dft_size;
        let dft_window_per_s = audio.sample_rate() as f32 / dft_size as f32;
        let dft_min_fq = dft_window_per_s * 1f32;
        let dft_max_fq = dft_window_per_s * dft_size as f32 / 2f32;
        log::info!("DFT can analyze frequencies in the range: {dft_min_fq} hz - {dft_max_fq} hz");

        let frequency_band_borders = [16, 60, 250, 500, 2000, 4000, 6000, 22000];
        let frequency_band_border_indices = frequency_band_borders
            .map(|frequency| dft_index_of_frequency(frequency, audio.sample_rate(), dft_size));

        Self {
            epoch: Instant::now(),
            audio,
            available_samples: 0,
            avg_available_samples: 44100f32 / 60f32,
            avg_available_samples_alpha: 0.95,
            signal_dft: Dft::new(args.dft_size),
            low_pass: LowPass::new(audio_buffer_size, 0.02),
            low_pass_dft: Dft::new(args.dft_size),
            high_pass: HighPass::new(audio_buffer_size, 0.1),
            high_pass_dft: Dft::new(args.dft_size),
            read_index: 0,
            write_index: 0,
            buf_size: 0,
            frequency_band_border_indices,
            beat_analysis: BeatAnalysis::new(),
            broadcast,
        }
    }

    /// Compute the read index (start of data to read), write index (index at which new data will
    /// be written (end of data to read) and the size of the ring buffer.
    fn compute_data_indices(&mut self) -> (usize, usize, usize) {
        let read_index = self.low_pass.write_index;
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

    pub fn tick(&mut self) {
        let (read_index, write_index, buf_size) = self.compute_data_indices();

        if write_index < read_index {
            for index in read_index..buf_size {
                let x = self.audio.signal.data[index];
                self.low_pass.sample(x);
                self.high_pass.sample(x);
            }
            for index in 0..write_index {
                let x = self.audio.signal.data[index];
                self.low_pass.sample(x);
                self.high_pass.sample(x);
            }
        } else {
            for index in read_index..write_index {
                let x = self.audio.signal.data[index];
                self.low_pass.sample(x);
                self.high_pass.sample(x);
            }
        }

        self.audio
            .signal
            .write_to_buffer(self.signal_dft.get_input_vec());
        self.signal_dft.run_transform();
        self.low_pass
            .write_to_buffer(self.low_pass_dft.get_input_vec());
        self.low_pass_dft.run_transform();
        self.high_pass
            .write_to_buffer(self.high_pass_dft.get_input_vec());
        self.high_pass_dft.run_transform();

        let beat_dft = &self.signal_dft;
        let beat_dft_lower = dft_index_of_frequency(35, self.audio.sample_rate(), beat_dft.size());
        let beat_dft_upper = dft_index_of_frequency(125, self.audio.sample_rate(), beat_dft.size());
        let beat_dft_sum_size = beat_dft_upper - beat_dft_lower;
        let bass_frequencies = &beat_dft.simple[beat_dft_lower..beat_dft_upper];

        if let Some(broadcast) = &self.broadcast {
            broadcast
                .send(bass_frequencies.to_owned())
                .expect("Failed to broadcast frame bass frequencies");
        }

        let beat_dft_sum = bass_frequencies.iter().fold(0f32, |a, b| a + b);
        self.beat_analysis
            .sample(beat_dft_sum / beat_dft_sum_size as f32);
    }
}
