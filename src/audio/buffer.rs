use std::{cell::UnsafeCell, sync::Arc};

pub const AUDIO_BUFFER_SIZE: usize = 44100 * 4;

pub struct BufferData {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    pub write_index: usize,
}

pub struct Buffer {
    data: UnsafeCell<BufferData>,
}

impl Buffer {
    pub fn new() -> Arc<Self> {
        let data = UnsafeCell::new(BufferData {
            left: vec![0.0; AUDIO_BUFFER_SIZE],
            right: vec![0.0; AUDIO_BUFFER_SIZE],
            write_index: 0,
        });

        Arc::new(Buffer { data })
    }

    pub fn read(&self) -> &BufferData {
        unsafe { self.data.get().as_ref() }.unwrap()
    }

    #[allow(clippy::mut_from_ref)]
    fn write(&self) -> &mut BufferData {
        unsafe { self.data.get().as_mut() }.unwrap()
    }

    pub fn write_samples(&self, samples: &[f32]) {
        let data = self.write();
        let base_index = data.write_index;
        let num_channels = 2;
        let num_samples = samples.len() / num_channels;
        let space_at_end = AUDIO_BUFFER_SIZE - base_index;

        let ref mut left = data.left;
        let ref mut right = data.right;

        for (index, channels) in samples.chunks(num_channels).take(space_at_end).enumerate() {
            left[base_index + index] = channels[0];
            right[base_index + index] = channels[1];
        }
        for (index, channels) in samples.chunks(num_channels).skip(space_at_end).enumerate() {
            left[index] = channels[0];
            right[index] = channels[1];
        }

        data.write_index = (data.write_index + num_samples) % AUDIO_BUFFER_SIZE;
    }

    pub fn write_to_buffer(&self, buffer: &mut [f32]) {
        let size = buffer.len();
        let data = self.read();

        // Mod buffer size to wrap back around.
        let start_index = (data.write_index + AUDIO_BUFFER_SIZE - size) % AUDIO_BUFFER_SIZE;
        let samples_to_end = AUDIO_BUFFER_SIZE - start_index;
        let samples_from_end = samples_to_end.min(size);

        // Copy start part of buffer (end may wrap around to the beginning of the audio buffer.
        buffer[..samples_from_end].copy_from_slice(&data.left[start_index..start_index + samples_from_end]);
    }
}

unsafe impl Sync for Buffer {}
