use crate::{dft::Dft, ring_buffer::RingBuffer};

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

    // Whether there is currentle an extraordinary signal energy.
    pub is_high: bool,
    pub is_beat: bool,

    // BPM detection.
    pub time_series: RingBuffer<f32>,
    pub dft: Dft,
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
    pub fn new(short_avg_size: usize, long_avg_size: usize) -> Self {
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
            time_series: RingBuffer::new(4 * 60),
            dft: Dft::new(4 * 60),
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

    fn update_bpm(&mut self, x: f32) {
        // self.time_series.push(if self.is_high { 1.0 } else { -1.0 });
        self.time_series.push(x);
        let dft = &mut self.dft;
        self.time_series.write_to_buffer(dft.get_input_vec());
        dft.run_transform();

        let mut peaks = Vec::with_capacity(30);
        let data = &dft.simple;

        let mut a = 0f32;
        let mut b = data[1];
        for i in 2..data.len() / 2 {
            let c = data[i];
            if a < b && b > c {
                peaks.push((i - 1, b));
            }

            a = b;
            b = c;
        }

        let min_dist = 3;

        let peak_indices = 0..peaks.len();
        let peak_indices = peak_indices
            .filter_map(|index| {
                let (this_peak_index, this_peak_value) = peaks[index];

                if index > 0 {
                    let (prev_peak_index, prev_peak_value) = peaks[index - 1];
                    let too_close = prev_peak_index >= this_peak_index - min_dist;
                    let too_small = this_peak_value < prev_peak_value;
                    if too_close && too_small {
                        return None;
                    }
                }

                if index < peaks.len() - 1 {
                    let (next_peak_index, next_peak_value) = peaks[index + 1];
                    let too_close = next_peak_index <= this_peak_index + min_dist;
                    let too_small = this_peak_value < next_peak_value;
                    if too_close && too_small {
                        return None;
                    }
                }

                let bpm = 60f32 * this_peak_index as f32 / 8f32;

                Some((bpm, this_peak_value))
            })
            .collect::<Vec<_>>();

        println!("{:?}", peak_indices);

        // let min_dist = 2;
    }

    pub fn sample(&mut self, x: f32) {
        self.update_averages(x);
        self.decide_beat(x);
        self.update_bpm(x);
    }
}
