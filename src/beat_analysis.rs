use std::time;

use log::debug;

pub struct BeatAnalysis {
    noise_threshold_factor: f32,
    beat_sigma_threshold_factor: f32,

    last_values: Vec<f32>,
    write_index: usize,

    // TODO maybe a floating average is better for the long_sum.
    long_sum_size: usize,
    long_sum: f32,

    short_sum_size: usize,
    short_sum: f32,
    square_sum: f32,

    standard_deviation: f32,

    // Whether there is currentle an extraordinary signal energy.
    pub is_beat: bool,
    pub matches_bpm: bool,
    // Absolute beat count.
    pub beat_count: usize,

    // Timestamps of the last N detected beats.
    beat_timestamps: Vec<time::Instant>,
    // Estimated BPM, but in seconds-per-beat format.
    spb: f32,
}

fn wrap_index(pos_offset: usize, neg_offset: usize, len: usize) -> usize {
    let idx = pos_offset + len - neg_offset;
    if idx >= len {
        idx % len
    } else {
        idx
    }
}

/// Relative difference.
/// For reference see <https://en.wikipedia.org/wiki/Relative_change_and_difference>
fn relative_delta(a: f32, b: f32) -> f32 {
    ((a / b) - 1f32).abs()
}

impl BeatAnalysis {
    pub fn new(short_avg_size: usize, long_avg_size: usize) -> Self {
        let beat_timestamps = (0..30)
            .rev()
            .map(|n| {
                time::Instant::now()
                    .checked_sub(time::Duration::from_secs(n))
                    .unwrap()
            })
            .collect::<Vec<_>>();
        Self {
            noise_threshold_factor: 0.25,
            beat_sigma_threshold_factor: 2.2,
            last_values: vec![0f32; long_avg_size],
            write_index: 0,
            long_sum_size: long_avg_size,
            long_sum: 0f32,
            short_sum_size: short_avg_size,
            short_sum: 0f32,
            square_sum: 0f32,
            standard_deviation: 0f32,
            is_beat: false,
            matches_bpm: false,
            beat_count: 0,
            beat_timestamps,
            spb: 0.1666,
        }
    }

    pub fn sample(&mut self, x: f32) {
        let buf_len = self.last_values.len();

        let long_sum_read_index = wrap_index(self.write_index, self.long_sum_size, buf_len);
        let long_sum_read_value = self.last_values[long_sum_read_index];
        self.long_sum = self.long_sum - long_sum_read_value + x;
        let long_avg = self.long_sum / self.long_sum_size as f32;

        let short_sum_read_index = wrap_index(self.write_index, self.short_sum_size, buf_len);
        let short_sum_read_value = self.last_values[short_sum_read_index];
        self.short_sum = self.short_sum - short_sum_read_value + x;
        let short_avg = self.short_sum / self.short_sum_size as f32;

        let square_sum_read_value = short_sum_read_value.powf(2f32);
        self.square_sum = self.square_sum - square_sum_read_value + x.powf(2f32);
        let square_avg = self.square_sum / self.short_sum_size as f32;

        self.standard_deviation = (square_avg - short_avg.powf(2f32)).sqrt();

        self.last_values[self.write_index] = x;
        self.write_index = wrap_index(self.write_index + 1, 0, buf_len);

        let not_noise = short_avg > self.noise_threshold_factor * long_avg;
        let loud_outlier =
            x > short_avg + self.beat_sigma_threshold_factor * self.standard_deviation;

        let was_beat = self.is_beat;
        self.is_beat = not_noise && loud_outlier;

        // Register the beat on the rising edge.
        if !was_beat && self.is_beat {
            let len = self.beat_timestamps.len();
            let idx = wrap_index(self.beat_count, 0, len);
            self.beat_timestamps[idx] = time::Instant::now();
            self.beat_count += 1;

            // Group beat deltas into clusters with 10% relative margin.
            let mut clusters: Vec<(f32, usize)> = Vec::new();
            for index in 0..(len - 1) {
                let a = self.beat_timestamps[wrap_index(self.beat_count + index, 0, len)];
                let b = self.beat_timestamps[wrap_index(self.beat_count + index + 1, 0, len)];
                let delta = (b - a).as_secs_f32();

                if let Some((sum, count)) = clusters.iter_mut().find(|(sum, count)| {
                    let center = *sum / *count as f32;
                    relative_delta(center, delta) < 0.1
                }) {
                    // Vote for this cluster.
                    *sum += delta;
                    *count += 1;
                } else {
                    // This branch is reached if no cluster matches.
                    clusters.push((delta, 1));
                }
            }

            // Select the cluster with the most entries.
            let (spb_sum, count) =
                clusters
                    .into_iter()
                    .fold((f32::INFINITY, 0), |a, b| if a.1 > b.1 { a } else { b });

            // Compute its center.
            let prev_bps = self.bps();
            self.spb = spb_sum / count as f32;
            let bps = self.bps();

            if relative_delta(prev_bps, bps) > 0.2 {
                debug!("BPS {prev_bps:.2} -> {bps:.2}");
            }
        }

        // The current time matches the beat, if the distance to the last or the next beat is
        // smaller than a fraction of the duration between beats.
        let last_beat = self.last_beat();
        let now = time::Instant::now();
        let next_beat = self.next_beat();
        let last_dist = (now - last_beat).as_secs_f32();
        let next_dist = (now.max(next_beat) - now.min(next_beat)).as_secs_f32();
        let beat_dist = last_dist.min(next_dist);
        self.matches_bpm = beat_dist / self.spb < 0.025;
    }

    fn bps(&self) -> f32 {
        60f32 / self.spb
    }

    pub fn last_beat(&self) -> time::Instant {
        self.beat_timestamps[wrap_index(self.beat_count, 1, self.beat_timestamps.len())]
    }

    pub fn next_beat(&self) -> time::Instant {
        self.last_beat() + time::Duration::from_secs_f32(self.spb)
    }
}
