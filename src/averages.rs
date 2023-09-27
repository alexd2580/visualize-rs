use std::ops::{Deref, DerefMut};

use crate::{ring_buffer::RingBuffer, utils::mix};

pub struct History {
    pub values: RingBuffer<f32>,
    pub min: f32,
    pub max: f32,
}

impl Deref for History {
    type Target = RingBuffer<f32>;

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl DerefMut for History {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}

impl History {
    pub fn new(num_samples: usize) -> Self {
        Self {
            values: RingBuffer::new(num_samples),
            min: 0f32,
            max: 0f32,
        }
    }
}

pub struct AlphaAvg {
    pub alpha: f32,
    pub avg: f32,
}

impl AlphaAvg {
    pub fn new(alpha: f32) -> Self {
        Self { alpha, avg: 0f32 }
    }

    pub fn sample(&mut self, x: f32) {
        self.avg = mix(self.avg, x, self.alpha);
    }
}

pub struct WindowedAvg {
    pub size: usize,

    sum: f32,
    pub avg: f32,

    square_sum: f32,
    square_avg: f32,

    pub sd: f32,
}

impl WindowedAvg {
    pub fn new(size: usize) -> Self {
        Self {
            size,
            sum: 0f32,
            avg: 0f32,
            square_sum: 0f32,
            square_avg: 0f32,
            sd: 0f32,
        }
    }

    pub fn sample(&mut self, old_x: f32, x: f32) {
        self.sum += x - old_x;
        self.avg = self.sum / self.size as f32;

        self.square_sum += x.powf(2f32) - old_x.powf(2f32);
        self.square_avg = self.square_sum / self.size as f32;

        self.sd = (self.square_avg - self.avg.powf(2f32)).sqrt();
    }
}

pub struct MaxDecay {
    alpha: f32,
    min_max: f32,
    pub max: f32,
}

impl MaxDecay {
    pub fn new(alpha: f32, min_max: f32) -> Self {
        Self {
            alpha,
            min_max,
            max: 0f32,
        }
    }

    pub fn sample(&mut self, x: f32) {
        self.max = (self.max * self.alpha).max(x).max(self.min_max);
    }
}
