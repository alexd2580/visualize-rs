use std::{f32::consts::PI, ffi::c_void, mem, sync::Arc};

use realfft::{RealFftPlanner, RealToComplex};
use rustfft::num_complex::Complex;

pub struct Dft {
    r2c: Arc<dyn RealToComplex<f32>>,

    hamming: Vec<f32>,

    input: Vec<f32>,
    scratch: Vec<Complex<f32>>,
    output: Vec<Complex<f32>>,
}

impl Dft {
    pub fn output_byte_size(input_size: usize) -> usize {
        (input_size / 2 + 1) * mem::size_of::<Complex<f32>>()
    }

    pub fn new(length: usize) -> Self {
        let mut real_planner = RealFftPlanner::<f32>::new();
        let r2c = real_planner.plan_fft_forward(length);

        let input = r2c.make_input_vec();
        let scratch = r2c.make_scratch_vec();
        let output = r2c.make_output_vec();

        assert_eq!(input.len(), length);
        // assert_eq!(scratch.len(), length);
        assert_eq!(output.len(), length / 2 + 1);

        let mut hamming = vec![0f32; length];
        for (index, val) in hamming.iter_mut().enumerate() {
            *val = 0.54 - (0.46 * (2f32 * PI * (index as f32 / (length - 1) as f32)).cos());
            // debug!("{}", *val);
        }

        Dft {
            r2c,
            hamming,
            input,
            scratch,
            output,
        }
    }

    pub fn get_input_vec(&mut self) -> &mut [f32] {
        &mut self.input
    }

    pub fn write_to_pointer(&self, target: *mut c_void) {
        unsafe {
            let size = self.output.len() as u32;
            *target.cast() = size;
            let target = target.add(mem::size_of::<i32>());

            self.output.as_ptr().copy_to(target.cast(), size as usize);
        }
    }

    pub fn apply_hamming(&mut self) {
        // let mut min = f32::INFINITY;
        // let mut max = f32::NEG_INFINITY;
        for (val, factor) in self.input.iter_mut().zip(self.hamming.iter()) {
            // min = min.min(*val);
            // max = max.max(*val);
            *val *= factor;
        }
        // debug!("inp {min:.2} {max:.2}");
    }

    pub fn run_transform(&mut self) {
        self.r2c
            .process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch)
            .unwrap();

        // let mut minx = f32::INFINITY;
        // let mut miny = f32::INFINITY;
        // let mut maxx = f32::NEG_INFINITY;
        // let mut maxy = f32::NEG_INFINITY;
        // for val in self.output.iter() {
        //     minx = minx.min(val.re);
        //     maxx = maxx.max(val.re);
        //     miny = miny.min(val.im);
        //     maxy = maxy.max(val.im);
        // }
        // debug!("dft {minx:.2} {maxx:.2} {miny:.2} {maxy:.2}");
    }

    pub fn apply_scaling(&mut self) {
        // Experimentally determined factor, scales the majority of frequencies to [0..1].
        let factor = 1f32 / (0.27 * self.input.len() as f32);
        for val in self.output.iter_mut() {
            *val *= factor;
        }
    }
}
