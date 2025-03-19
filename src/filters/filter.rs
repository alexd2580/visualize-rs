pub trait Filter {
    fn sample(&mut self, x: f32) -> f32;
}
