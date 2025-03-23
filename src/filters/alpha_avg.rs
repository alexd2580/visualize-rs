use crate::utils::mix;

use super::filter::Filter;

pub struct AlphaAvg {
    alpha: f32,
    pub avg: f32,
}

impl AlphaAvg {
    pub fn new_with_value(alpha: f32, avg: f32) -> Self {
        Self { alpha, avg }
    }

    pub fn new(alpha: f32) -> Self {
        Self::new_with_value(alpha, 0.0)
    }
}

impl Filter for AlphaAvg {
    fn sample(&mut self, x: f32) -> f32 {
        self.avg = mix(self.avg, x, self.alpha);
        self.avg
    }
}
