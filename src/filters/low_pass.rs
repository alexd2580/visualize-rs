use super::filter::Filter;

pub struct LowPass {
    alpha: f32,
    last: f32,
}

impl LowPass {
    #[allow(dead_code)]
    pub fn new(sample_rate: f32, cutoff_fq: usize) -> Self {
        let dt = 1.0 / sample_rate;
        let tau = 1.0 / (2.0 * std::f32::consts::PI * cutoff_fq as f32);
        let alpha = dt / (tau + dt);

        LowPass { alpha, last: 0.0 }
    }
}

impl Filter for LowPass {
    fn sample(&mut self, x: f32) -> f32 {
        self.last = self.alpha * x + (1f32 - self.alpha) * self.last;
        self.last
    }
}
