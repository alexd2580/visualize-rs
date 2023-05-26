use std::{ops::Deref, rc::Rc};

use log::debug;

use ash;

use crate::error::Error;

pub struct Entry {
    pub entry: ash::Entry,
}

impl Deref for Entry {
    type Target = ash::Entry;

    fn deref(&self) -> &Self::Target {
        &self.entry
    }
}

impl Entry {
    pub fn new() -> Result<Rc<Self>, Error> {
        debug!("Creating entry");
        let entry = ash::Entry::linked();
        Ok(Rc::new(Entry { entry }))
    }
}
