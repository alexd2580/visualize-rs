use std::{
    cell::UnsafeCell,
    collections::BTreeMap,
    ops::Bound::{Excluded, Included},
    sync::{Arc, RwLock},
};

const AUDIO_BUFFER_SIZE: usize = 44100 * 4;

struct AudioBufferData {
    pub left: [f32; AUDIO_BUFFER_SIZE],
    pub right: [f32; AUDIO_BUFFER_SIZE],
    pub write_index: usize,
}

struct AudioBufferLocks {
    ranges: BTreeMap<usize, usize>,
}

impl AudioBufferLocks {
    fn new() -> Self {
        AudioBufferLocks {
            ranges: BTreeMap::new(),
        }
    }

    fn try_lock(&mut self, pos: usize, len: usize) -> bool {
        if let Some((r_pos, r_len)) = self.ranges.range((Included(0), Excluded(pos))).rev().next() {
            if r_pos + r_len > pos {
                return false;
            }
        }
        if let Some((r_pos, _)) = self
            .ranges
            .range((Included(pos), Excluded(AUDIO_BUFFER_SIZE)))
            .next()
        {
            if pos + len > *r_pos {
                return false;
            }
        }

        self.ranges.insert(pos, len);
        true
    }

    fn unlock(&mut self, pos: usize) {
        self.ranges.remove(&pos);
    }
}

#[cfg(test)]
mod tests {
    use super::AudioBufferLocks;

    #[test]
    fn cannot_double_lock_range() {
        let mut audio_buffer_locks = AudioBufferLocks::new();
        assert!(audio_buffer_locks.try_lock(10, 10));
        assert!(!audio_buffer_locks.try_lock(10, 10));

        assert!(audio_buffer_locks.try_lock(20, 10));
        assert!(!audio_buffer_locks.try_lock(20, 10));

        assert!(!audio_buffer_locks.try_lock(15, 1));
        assert!(!audio_buffer_locks.try_lock(15, 10));
        assert!(!audio_buffer_locks.try_lock(25, 1));

        audio_buffer_locks.unlock(10);
        assert!(audio_buffer_locks.try_lock(15, 1));

        audio_buffer_locks.unlock(20);
        assert!(audio_buffer_locks.try_lock(25, 1));

        assert!(audio_buffer_locks.try_lock(16, 9));

        assert!(!audio_buffer_locks.try_lock(0, 40));
    }
}

pub struct AudioBuffer {
    data: UnsafeCell<AudioBufferData>,
    locks: RwLock<AudioBufferLocks>,
}

impl AudioBuffer {
    pub fn new() -> Arc<Self> {
        let data = UnsafeCell::new(AudioBufferData {
            left: [0.0; AUDIO_BUFFER_SIZE],
            right: [0.0; AUDIO_BUFFER_SIZE],
            write_index: 0,
        });

        let locks = RwLock::new(AudioBufferLocks::new());

        Arc::new(AudioBuffer { data, locks })
    }
}

unsafe impl Sync for AudioBuffer {}
