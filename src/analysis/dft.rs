use std::{f32::consts::PI, ffi::c_void, mem, sync::Arc};

use realfft;
use rustfft::num_complex::Complex;

pub struct Dft {
    r2c: Arc<dyn realfft::RealToComplex<f32>>,
    // c2r: Arc<dyn realfft::ComplexToReal<f32>>,
    blackman_harris: Vec<f32>,

    pub input: Vec<f32>,
    scratch: Vec<Complex<f32>>,
    output: Vec<Complex<f32>>,

    fq_decay: Vec<f32>,
    fq_db: Vec<f32>,

    num_bins: usize,
    bin_indices: Vec<(usize, usize)>,
}

impl Dft {
    pub fn output_byte_size(input_size: usize) -> usize {
        (input_size / 2 + 1) * mem::size_of::<f32>()
    }

    pub fn new(length: usize, sample_rate: f32) -> Self {
        let mut real_planner = realfft::RealFftPlanner::<f32>::new();
        let r2c = real_planner.plan_fft_forward(length);
        // let c2r = real_planner.plan_fft_inverse(length);

        let input = r2c.make_input_vec();
        let scratch = r2c.make_scratch_vec();
        let output = r2c.make_output_vec();

        assert_eq!(input.len(), length);
        // assert_eq!(scratch.len(), length);
        assert_eq!(output.len(), length / 2 + 1);

        let a0 = 0.35875;
        let a1 = 0.48829;
        let a2 = 0.14128;
        let a3 = 0.01168;
        let m = (length - 1) as f32;

        let mut blackman_harris = vec![0f32; length];
        for (index, val) in blackman_harris.iter_mut().enumerate() {
            *val = a0 - a1 * (2.0 * PI * index as f32 / m).cos()
                + a2 * (4.0 * PI * index as f32 / m).cos()
                - a3 * (6.0 * PI * index as f32 / m).cos();
        }

        let fq_decay = vec![0.0; length / 2 + 1];
        let fq_db = vec![0.0; length / 2 + 1];

        let num_bins = 60;
        let bin_fq_step = sample_rate / length as f32;

        let min_fq = 20.0f32;
        let max_fq = 20_000.0;

        let exp_base = max_fq / min_fq;
        let exp_step = 1.0 / num_bins as f32;

        let bin_borders = (0..num_bins + 1)
            .map(|bin_index| {
                let exp_fq = min_fq * exp_base.powf(bin_index as f32 * exp_step);
                exp_fq / bin_fq_step
            })
            .collect::<Vec<_>>();
        let bin_indices = bin_borders
            .iter()
            .take(num_bins)
            .zip(bin_borders.iter().skip(1))
            .map(|(i1, i2)| (i1.floor() as usize, i2.ceil() as usize))
            .collect();

        Dft {
            r2c,
            // c2r,
            blackman_harris,
            input,
            scratch,
            output,
            fq_decay,
            fq_db,
            num_bins,
            bin_indices,
        }
    }

    pub fn size(&self) -> usize {
        self.input.len()
    }

    pub fn get_input_vec(&mut self) -> &mut [f32] {
        &mut self.input
    }

    // pub fn write_input_to_pointer(&self, target: *mut c_void) {
    //     unsafe {
    //         let size = self.input.len();
    //         *target.cast::<u32>() = u32::try_from(size).unwrap();
    //         let target = target.add(mem::size_of::<i32>());
    //
    //         self.input.as_ptr().copy_to(target.cast(), size);
    //     }
    // }

    pub fn serialized_size(&self) -> usize {
        Dft::output_byte_size(self.size()) + mem::size_of::<i32>()
    }

    pub fn write_to_pointer(&self, target: *mut c_void) {
        unsafe {
            let size = self.fq_db.len();
            *target.cast::<u32>() = u32::try_from(size).unwrap();
            let target = target.add(mem::size_of::<i32>());

            self.fq_db.as_ptr().copy_to(target.cast(), size);
        }
    }

    pub fn log_bin_serialized_size(&self) -> usize {
        mem::size_of::<i32>() + self.num_bins * mem::size_of::<f32>()
    }

    pub fn write_log_bins_to_pointer(&self, target: *mut c_void) {
        unsafe {
            *target.cast::<u32>() = u32::try_from(self.num_bins).unwrap();
            let target = target.add(mem::size_of::<i32>());
            let target = target.cast::<f32>();

            for (index, (start, end)) in self.bin_indices.iter().enumerate() {
                let slice = &self.fq_db[*start..*end + 1];
                *target.add(index) = slice.iter().sum::<f32>() / slice.len() as f32;
            }
        }
    }

    pub fn run_transform(&mut self) {
        // Hamming window for smoother DFT results.
        for (val, factor) in self.input.iter_mut().zip(self.blackman_harris.iter()) {
            *val *= factor;
        }

        self.r2c
            .process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch)
            .unwrap();

        for i in 0..self.output.len() {
            let old = self.fq_decay[i];
            self.fq_decay[i] = (0.2 * self.output[i].norm() + 0.8 * old); // .max(0.8 * old);
            self.fq_db[i] = 20.0 * self.fq_decay[i].max(1e-6).log10();
        }

        // Experimentally determined factor, scales the majority of frequencies to [0..1].
        // let factor = 1f32 / (0.27 * self.input.len() as f32);
        // for (&output, simple) in self.output.iter().zip(self.simple.iter_mut()) {
        //     let next_val = output.norm() * factor;
        //     *simple = 0f32.max(*simple - 0.015).max(next_val);
        // }
    }

    // fn run_inverse(&mut self) {
    //     self.c2r
    //         .process_with_scratch(&mut self.output, &mut self.input, &mut self.scratch)
    //         .unwrap();
    // }

    // pub fn autocorrelate(&mut self) {
    //     self.run_transform();
    //     for x in self.output.iter_mut() {
    //         *x = *x * x.conj();
    //     }
    //     self.run_inverse();
    // }
}
