use std::{ffi::c_void, mem};

pub struct RingBuffer<T> {
    pub size: usize,
    pub data: Vec<T>,
    pub prev_index: usize,
    pub write_index: usize,
}

impl<T: Clone + Copy + Default> RingBuffer<T> {
    pub fn new(size: usize) -> Self {
        RingBuffer {
            size,
            data: vec![T::default(); size],
            prev_index: size - 1,
            write_index: 0,
        }
    }

    fn unwrap(&self) -> (&[T], &[T]) {
        (
            &self.data.as_slice()[self.write_index..],
            &self.data.as_slice()[..self.write_index],
        )
    }

    pub fn advance(&mut self) {
        self.prev_index = self.write_index;
        self.write_index = if self.write_index == self.size - 1 {
            0
        } else {
            self.write_index + 1
        };
    }

    pub fn last(&self) -> T {
        self.data[self.prev_index]
    }

    pub fn push(&mut self, x: T) {
        self.data[self.write_index] = x;
        self.advance();
    }

    pub fn write_to_buffer(&self, buffer: &mut [T]) {
        let (start, end) = self.unwrap();
        let size = buffer.len();

        let from_end = end.len().min(size);
        let from_start = size - from_end;

        buffer[..from_start].copy_from_slice(&start[start.len() - from_start..]);
        buffer[from_start..].copy_from_slice(&end[end.len() - from_end..]);
    }

    /// Read chronologically from `read_pos` until the latest written audio sample: `write_pos`.
    pub fn iter_at(&self, read_index: usize) -> impl Iterator<Item = &T> {
        let write_index = self.write_index;
        let buffer = self.data.as_slice();
        let (init_end_index, tail_end_index) = if read_index < write_index {
            (write_index, 0)
        } else {
            (buffer.len(), write_index)
        };
        buffer[read_index..init_end_index]
            .iter()
            .chain(buffer[0..tail_end_index].iter())
    }

    /// Write the rinbuffer to the pointer, posting its size and write index first and then
    /// updating only the section that has been modified, namely `[read_index..(potential
    /// wraparound)..write_index]`.
    pub fn write_to_pointer(&self, read_index: usize, target: *mut c_void) {
        unsafe {
            let size = self.size as u32;
            *target.cast() = size;
            let target = target.add(mem::size_of::<u32>());

            let write_index = self.write_index as u32;
            *target.cast() = write_index;
            let target = target.add(mem::size_of::<u32>());

            if read_index <= self.write_index {
                let data = &self.data.as_slice()[read_index..self.write_index];
                let target = target.add(read_index * mem::size_of::<f32>());
                let count = self.write_index - read_index;
                data.as_ptr().copy_to(target.cast(), count);
            } else {
                // Start part: 0 .. write_index
                let data = &self.data.as_slice()[..self.write_index];
                data.as_ptr().copy_to(target.cast(), self.write_index);

                // End part: read_index .. size
                let data = &self.data.as_slice()[read_index..];
                let target = target.add(read_index * mem::size_of::<f32>());
                let count = self.size - read_index;
                data.as_ptr().copy_to(target.cast(), count);
            }
        }
    }
}
