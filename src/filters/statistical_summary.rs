use crate::ring_buffer::RingBuffer;

use super::filter::Filter;

#[derive(PartialEq)]
pub enum Grade {
    Low,
    Normal,
    High,
}

pub struct StatisticalSummary {
    size: usize,
    buffer: RingBuffer<f32>,

    sum: f32,
    pub avg: f32,

    total_energy: f32,
    pub energy: f32,

    pub sd: f32,
}

impl StatisticalSummary {
    pub fn new(size: usize) -> Self {
        Self {
            size,
            buffer: RingBuffer::new_with_default(size, 0.0),
            sum: 0f32,
            avg: 0f32,
            total_energy: 0f32,
            energy: 0f32,
            sd: 0f32,
        }
    }

    pub fn grade(&self, x: f32) -> Grade {
        if x < self.avg - 0.5 * self.sd {
            Grade::Low
        } else if x > self.avg + 0.5 * self.sd {
            Grade::High
        } else {
            Grade::Normal
        }
    }
}

impl Filter for StatisticalSummary {
    fn sample(&mut self, x: f32) -> f32 {
        let old_x = self.buffer.data[self.buffer.write_index];
        self.buffer.push(x);
        self.sum += x - old_x;
        self.avg = self.sum / self.size as f32;

        self.total_energy += x.powi(2) - old_x.powi(2);
        self.energy = self.total_energy / self.size as f32;

        self.sd = (self.energy - self.avg.powi(2)).sqrt();
        self.sd
    }
}
