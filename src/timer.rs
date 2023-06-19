use std::{collections::HashMap, time};

use log::info;

use crate::utils::mix;

pub struct Timer {
    alpha: f32,
    last_section_end: time::Instant,
    section_order: Vec<*const u8>,
    sections: HashMap<*const u8, (&'static str, f32, f32)>,
}

impl Timer {
    pub fn new(alpha: f32) -> Timer {
        Timer {
            alpha,
            last_section_end: time::Instant::now(),
            section_order: Vec::new(),
            sections: HashMap::new(),
        }
    }

    pub fn start(&mut self) {
        self.last_section_end = time::Instant::now();
    }

    pub fn section(&mut self, name: &'static str) {
        let delta = self.last_section_end.elapsed().as_secs_f32();
        let key = name.as_ptr();
        match self.sections.get_mut(&key) {
            None => {
                self.sections.insert(key, (name, delta, delta.powf(2f32)));
                self.section_order.push(key);
            }
            Some((_, avg_delta, avg_square_delta)) => {
                *avg_delta = mix(*avg_delta, delta, self.alpha);
                *avg_square_delta = mix(*avg_square_delta, delta.powf(2f32), self.alpha);
            }
        }

        self.start();
    }

    pub fn print(&self) {
        info!("Timings");
        for key in &self.section_order {
            let (name, avg_delta, avg_square_delta) = self.sections.get(key).unwrap();
            let variance = (avg_square_delta - avg_delta.powf(2f32)) * 1000f32;
            let avg_delta = avg_delta * 1000f32;
            info!("  {name: <20} {avg_delta:>10.2}ms (s²: {variance:>10.2}ms)");
        }
    }
}
