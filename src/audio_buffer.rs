use std::{cell::UnsafeCell, sync::Arc};

pub const AUDIO_BUFFER_SIZE: usize = 44100 * 4;

pub struct AudioBufferData {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    pub write_index: usize,
}

pub struct AudioBuffer {
    data: UnsafeCell<AudioBufferData>,
}

impl AudioBuffer {
    pub fn new() -> Arc<Self> {
        let data = UnsafeCell::new(AudioBufferData {
            left: vec![0.0; AUDIO_BUFFER_SIZE],
            right: vec![0.0; AUDIO_BUFFER_SIZE],
            write_index: 0,
        });

        Arc::new(AudioBuffer { data })
    }

    pub fn read(&self) -> &AudioBufferData {
        unsafe { self.data.get().as_ref() }.unwrap()
    }

    #[allow(clippy::mut_from_ref)]
    fn write(&self) -> &mut AudioBufferData {
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
}

unsafe impl Sync for AudioBuffer {}
