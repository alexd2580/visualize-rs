use std::ops::Deref;

use crate::ring_buffer::RingBuffer;

pub struct HighPass {
    buffer: RingBuffer<f32>,
    prev_sample: f32,
    alpha: f32,
}

impl Deref for HighPass {
    type Target = RingBuffer<f32>;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl HighPass {
    pub fn new(size: usize, alpha: f32) -> Self {
        HighPass {
            buffer: RingBuffer::new(size),
            prev_sample: 0.0,
            alpha,
        }
    }

    pub fn sample(&mut self, x: f32) {
        self.buffer
            .push(self.alpha * (self.buffer.last() + x - self.prev_sample));
    }
}
