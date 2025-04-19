use crate::ring_buffer::RingBuffer;

use super::filter::Filter;

pub struct Energy {
    size: usize,
    buffer: RingBuffer<f32>,
    sum: f32,
    last: f32,
    cumulative: f32,
}

impl Energy {
    pub fn new(size: usize) -> Self {
        Self {
            size,
            buffer: RingBuffer::new_with_default(size, 0.0),
            sum: 0f32,
            last: 0f32,
            cumulative: 0f32,
        }
    }

    pub fn last(&self) -> f32 {
        self.last
    }

    pub fn cumulative(&self) -> f32 {
        self.cumulative
    }
}

impl Filter for Energy {
    fn sample(&mut self, x: f32) -> f32 {
        let old_x = self.buffer.oldest();
        self.buffer.push(x);

        self.sum += x.powi(2) - old_x.powi(2);
        self.last = self.sum / self.size as f32;
        self.cumulative += self.last;
        self.last
    }
}
