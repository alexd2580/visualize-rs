use crate::ring_buffer::RingBuffer;

pub struct Stereo {
    pub left: RingBuffer<f32>,
    pub right: RingBuffer<f32>,
}

impl Stereo {
    pub fn new(size: usize) -> Self {
        Stereo {
            left: RingBuffer::new(size),
            right: RingBuffer::new(size),
        }
    }

    pub fn write_samples(&mut self, samples: &[f32]) {
        let num_channels = 2;
        let num_samples = samples.len() / num_channels;
        let space_at_end = self.left.size - self.left.write_index;

        let left = &mut self.left.data;
        let right = &mut self.right.data;

        for (index, channels) in samples.chunks(num_channels).take(space_at_end).enumerate() {
            left[self.left.write_index + index] = channels[0];
            right[self.left.write_index + index] = channels[1];
        }
        for (index, channels) in samples.chunks(num_channels).skip(space_at_end).enumerate() {
            left[index] = channels[0];
            right[index] = channels[1];
        }

        let write_index = (self.left.write_index + num_samples) % self.left.size;
        self.left.write_index = write_index;
        self.right.write_index = write_index;
    }
}
