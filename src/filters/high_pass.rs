use std::f32::consts::PI;

use super::filter::Filter;

pub struct HighPass {
    alpha: f32,
    prev_in: f32,
    last: f32,
}

impl HighPass {
    pub fn new(sample_rate: usize, cutoff_fq: usize) -> Self {
        let dt = 1.0 / sample_rate as f32;
        let tau = 1.0 / (2.0 * PI * cutoff_fq as f32);
        let alpha = tau / (tau + dt); // Filter coefficient

        HighPass {
            alpha,
            prev_in: 0.0,
            last: 0.0,
        }
    }
}

impl Filter for HighPass {
    fn sample(&mut self, x: f32) -> f32 {
        self.last = self.alpha * (self.last + x - self.prev_in);
        self.prev_in = x;
        self.last
    }
}
