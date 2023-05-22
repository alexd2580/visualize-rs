use std::sync::Arc;

use realfft::{RealFftPlanner, RealToComplex};
use rustfft::num_complex::Complex;

pub struct Dft {
    r2c: Arc<dyn RealToComplex<f32>>,

    input: Vec<f32>,
    scratch: Vec<Complex<f32>>,
    output: Vec<Complex<f32>>,
}

impl Dft {
    pub fn new() -> Self {
        let length = 4096;

        let mut real_planner = RealFftPlanner::<f32>::new();
        let r2c = real_planner.plan_fft_forward(length);

        let input = r2c.make_input_vec();
        let scratch = r2c.make_scratch_vec();
        let output = r2c.make_output_vec();

        assert_eq!(input.len(), length);
        // assert_eq!(scratch.len(), length);
        assert_eq!(output.len(), length / 2 + 1);

        Dft { r2c, input, scratch, output }
    }

    pub fn get_input_vec(&mut self) -> &mut [f32] {
        &mut self.input
    }

    pub fn get_output_vec(&self) -> &[Complex<f32>] {
        &self.output
    }

    pub fn run_transform(&mut self) {
        self.r2c.process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch).unwrap();
    }
}
