use std::{f32::consts::PI, ffi::c_void, mem, sync::Arc};

use realfft;
use rustfft::num_complex::Complex;

pub struct Dft {
    r2c: Arc<dyn realfft::RealToComplex<f32>>,
    c2r: Arc<dyn realfft::ComplexToReal<f32>>,

    hamming: Vec<f32>,

    pub input: Vec<f32>,
    scratch: Vec<Complex<f32>>,
    output: Vec<Complex<f32>>,

    pub simple: Vec<f32>,
    pub simple_old: Vec<f32>,
}

impl Dft {
    pub fn output_byte_size(input_size: usize) -> usize {
        (input_size / 2 + 1) * mem::size_of::<f32>()
    }

    pub fn new(length: usize) -> Self {
        let mut real_planner = realfft::RealFftPlanner::<f32>::new();
        let r2c = real_planner.plan_fft_forward(length);
        let c2r = real_planner.plan_fft_inverse(length);

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

        let simple = vec![0.0; length / 2 + 1];
        let simple_old = vec![0.0; length / 2 + 1];

        Dft {
            r2c,
            c2r,
            hamming,
            input,
            scratch,
            output,
            simple,
            simple_old,
        }
    }

    pub fn size(&self) -> usize {
        self.input.len()
    }

    pub fn get_input_vec(&mut self) -> &mut [f32] {
        &mut self.input
    }

    pub fn write_input_to_pointer(&self, target: *mut c_void) {
        unsafe {
            let size = self.input.len();
            *target.cast::<u32>() = u32::try_from(size).unwrap();
            let target = target.add(mem::size_of::<i32>());

            self.input.as_ptr().copy_to(target.cast(), size);
        }
    }

    pub fn serialized_size(&self) -> usize {
        Dft::output_byte_size(self.size()) + mem::size_of::<i32>()
    }

    pub fn write_to_pointer(&self, target: *mut c_void) {
        unsafe {
            let size = self.simple.len();
            *target.cast::<u32>() = u32::try_from(size).unwrap();
            let target = target.add(mem::size_of::<i32>());

            self.simple.as_ptr().copy_to(target.cast(), size);
        }
    }

    pub fn run_transform(&mut self) {
        // Hamming window for smoother DFT results.
        for (val, factor) in self.input.iter_mut().zip(self.hamming.iter()) {
            *val *= factor;
        }

        self.r2c
            .process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch)
            .unwrap();

        std::mem::swap(&mut self.simple, &mut self.simple_old);

        for (&output, simple) in self.output.iter().zip(self.simple.iter_mut()) {
            *simple = output.norm();
        }

        // Experimentally determined factor, scales the majority of frequencies to [0..1].
        // let factor = 1f32 / (0.27 * self.input.len() as f32);
        // for (&output, simple) in self.output.iter().zip(self.simple.iter_mut()) {
        //     let next_val = output.norm() * factor;
        //     *simple = 0f32.max(*simple - 0.015).max(next_val);
        // }
    }

    fn run_inverse(&mut self) {
        self.c2r
            .process_with_scratch(&mut self.output, &mut self.input, &mut self.scratch)
            .unwrap();
    }

    pub fn autocorrelate(&mut self) {
        self.run_transform();
        for x in self.output.iter_mut() {
            *x = *x * x.conj();
        }
        self.run_inverse();
        // println!("{:?}", self.input);
    }
}
