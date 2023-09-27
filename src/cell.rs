use std::{
    cell::{Ref, RefCell, RefMut},
    clone::Clone,
    rc::Rc,
};

use crate::error::{Error, VResult};

pub struct Cell<T>(Rc<RefCell<T>>);

impl<T> Clone for Cell<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Cell<T> {
    pub fn new(value: T) -> Self {
        Self(Rc::new(RefCell::new(value)))
    }

    pub fn as_ref(&self) -> Ref<T> {
        self.0.borrow()
    }

    pub fn as_mut_ref(&self) -> RefMut<T> {
        self.0.borrow_mut()
    }

    pub fn set(&self, value: T) {
        *self.0.borrow_mut() = value;
    }

    pub fn into_inner(self) -> VResult<T> {
        Ok(Rc::into_inner(self.0)
            .ok_or_else(|| Error::Local("Still multiple references to cell present".to_string()))?
            .into_inner())
    }
}
