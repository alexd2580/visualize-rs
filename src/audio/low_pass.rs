use std::ops::Deref;

use crate::ring_buffer::RingBuffer;

pub struct LowPass {
    buffer: RingBuffer<f32>,
    alpha: f32,
}

impl Deref for LowPass {
    type Target = RingBuffer<f32>;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl LowPass {
    pub fn new(size: usize, alpha: f32) -> Self {
        LowPass {
            buffer: RingBuffer::new(size),
            alpha,
        }
    }

    pub fn sample(&mut self, x: f32) {
        self.buffer
            .push(self.alpha * x + (1f32 - self.alpha) * self.buffer.last());
    }
}
