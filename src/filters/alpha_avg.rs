use crate::utils::mix;

use super::filter::Filter;

pub struct AlphaAvg {
    pub alpha: f32,
    pub avg: f32,
}

impl AlphaAvg {
    pub fn new(alpha: f32) -> Self {
        Self { alpha, avg: 0f32 }
    }
}

impl Filter for AlphaAvg {
    fn sample(&mut self, x: f32) -> f32 {
        self.avg = mix(self.avg, x, self.alpha);
        self.avg
    }
}
