use super::filter::Filter;

pub struct MaxDecayNormalizer {
    alpha: f32,
    min_max: f32,
    pub max: f32,
}

impl MaxDecayNormalizer {
    pub fn new(alpha: f32, min_max: f32) -> Self {
        Self {
            alpha,
            min_max,
            max: 0f32,
        }
    }
}

impl Filter for MaxDecayNormalizer {
    fn sample(&mut self, x: f32) -> f32 {
        self.max = (self.max * self.alpha).max(x).max(self.min_max);
        x / self.max
    }
}
