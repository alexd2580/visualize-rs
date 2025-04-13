use std::{ffi::c_void, mem};

pub struct RingBuffer<T> {
    pub size: usize,
    pub data: Vec<T>,
    pub prev_index: usize,
    pub write_index: usize,
}

impl<T: Copy + Default> RingBuffer<T> {
    pub fn new(size: usize) -> Self {
        RingBuffer {
            size,
            data: vec![T::default(); size],
            prev_index: size - 1,
            write_index: 0,
        }
    }
}

impl<T: Copy> RingBuffer<T> {
    pub fn new_with_default(size: usize, default: T) -> Self {
        RingBuffer {
            size,
            data: vec![default; size],
            prev_index: size - 1,
            write_index: 0,
        }
    }

    pub fn new_with_data(data: Vec<T>) -> Self {
        let size = data.len();
        RingBuffer {
            size,
            data,
            prev_index: size - 1,
            write_index: 0,
        }
    }
}

impl<T: Copy> From<Vec<T>> for RingBuffer<T> {
    fn from(value: Vec<T>) -> Self {
        RingBuffer::new_with_data(value)
    }
}

impl<T: Copy> RingBuffer<T> {
    fn unwrap(&self) -> (&[T], &[T]) {
        (
            &self.data.as_slice()[self.write_index..],
            &self.data.as_slice()[..self.write_index],
        )
    }

    pub fn offset_index(&self, pos: usize, neg: usize) -> usize {
        let idx = self.write_index + pos + self.size - neg;
        if idx >= self.size {
            idx % self.size
        } else {
            idx
        }
    }

    pub fn at_offset(&self, pos: usize, neg: usize) -> &T {
        &self.data[self.offset_index(pos, neg)]
    }

    pub fn advance(&mut self) {
        self.prev_index = self.write_index;
        self.write_index = if self.write_index == self.size - 1 {
            0
        } else {
            self.write_index + 1
        };
    }

    pub fn oldest(&self) -> T {
        self.data[self.write_index]
    }

    pub fn prev(&self) -> T {
        self.data[self.prev_index]
    }

    pub fn push(&mut self, x: T) {
        self.data[self.write_index] = x;
        self.advance();
    }

    pub fn serialized_size(&self) -> usize {
        self.size * mem::size_of::<T>() + 2 * mem::size_of::<i32>()
    }

    pub fn write_to_buffer(&self, offset: isize, buffer: &mut [T]) {
        let (init, tail) = self.unwrap();

        let wanted_size = buffer.len();
        let init_size = init.len();

        let self_isize = self.size as isize;
        assert!(-self_isize <= offset && offset <= self_isize);

        let start = if offset < 0 {
            offset + self.size as isize
        } else {
            offset
        } as usize;
        let end = start + wanted_size;

        assert!(end <= self.size);

        let from_init = init_size.saturating_sub(start).min(wanted_size);
        let init_start = init_size.min(start);
        buffer[..from_init].copy_from_slice(&init[init_start..init_start + from_init]);

        let from_tail = wanted_size - from_init;
        let tail_start = start.saturating_sub(init_size);
        buffer[from_init..].copy_from_slice(&tail[tail_start..tail_start + from_tail]);
    }

    /// Write the ringbuffer to the pointer, posting its size and write index first and then
    /// updating only the section that has been modified, namely `[read_index..(potential
    /// wraparound)..write_index]`.
    pub fn write_to_pointer(&self, read_index: usize, write_index: usize, target: *mut c_void) {
        unsafe {
            *target.cast::<u32>() = u32::try_from(self.size).unwrap();
            let target = target.add(mem::size_of::<u32>());

            *target.cast::<u32>() = u32::try_from(write_index).unwrap();
            let target = target.add(mem::size_of::<u32>());

            if read_index < write_index {
                let data = &self.data.as_slice()[read_index..write_index];
                let target = target.add(read_index * mem::size_of::<f32>());
                let count = write_index - read_index;
                data.as_ptr().copy_to(target.cast(), count);
            } else {
                // Start part: 0 .. write_index
                let data = &self.data.as_slice()[..write_index];
                data.as_ptr().copy_to(target.cast(), write_index);

                // End part: read_index .. size
                let data = &self.data.as_slice()[read_index..];
                let target = target.add(read_index * mem::size_of::<f32>());
                let count = self.size - read_index;
                data.as_ptr().copy_to(target.cast(), count);
            }
        }
    }
}
