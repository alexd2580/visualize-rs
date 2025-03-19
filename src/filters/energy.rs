use crate::ring_buffer::RingBuffer;

use super::filter::Filter;

pub struct Energy {
    size: usize,
    buffer: RingBuffer<f32>,
    total_energy: f32,
}

impl Energy {
    pub fn new(size: usize) -> Self {
        Self {
            size,
            buffer: RingBuffer::new_with_default(size, 0.0),
            total_energy: 0f32,
        }
    }

    pub fn energy(&self) -> f32 {
        self.total_energy / self.size as f32
    }
}

impl Filter for Energy {
    fn sample(&mut self, x: f32) -> f32 {
        let old_x = self.buffer.data[self.buffer.write_index];
        self.buffer.push(x);

        self.total_energy += x.powi(2) - old_x.powi(2);
        self.total_energy
    }
}
