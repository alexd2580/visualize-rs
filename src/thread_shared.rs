use std::{cell::UnsafeCell, ops::Deref, sync::Arc};

/// Wrap the content in an unsafe cell implementing `Sync` on it.
/// Essentially allowing concurrent read and write which is WHAT WE WANT.
struct UnsafeData<Content>(UnsafeCell<Content>);

/// `Sync` required for this data to be thread-sharable.
unsafe impl<Content> Sync for UnsafeData<Content> {}

impl<Content> Deref for UnsafeData<Content> {
    type Target = UnsafeCell<Content>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct ThreadShared<Content>(Arc<UnsafeData<Content>>);

impl<Content> ThreadShared<Content> {
    pub fn new(content: Content) -> Self {
        ThreadShared(Arc::new(UnsafeData(UnsafeCell::new(content))))
    }

    pub fn read(&self) -> &Content {
        unsafe { self.0.get().as_ref() }.unwrap()
    }

    #[allow(clippy::mut_from_ref)]
    pub fn write(&self) -> &mut Content {
        unsafe { self.0.get().as_mut() }.unwrap()
    }
}

impl<Content> Clone for ThreadShared<Content> {
    fn clone(&self) -> Self {
        ThreadShared(self.0.clone())
    }
}
