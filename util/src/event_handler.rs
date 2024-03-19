use arc_swap::{ArcSwapOption, Guard};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct EventHandler<T: ?Sized> {
    // FIXME: it would be preferred if we didnt have to double allocate here
    // (type is ArcSwapAny<Option<Arc<Mutex<box<T>>>>) but since ArcSwaps implementation uses an
    // AtomicPtr (which does not support unsized types as there is not language support for atomic
    // operations larger than a word), it has to be sized for now.
    //
    // I(jumbeldliam) am also unsure if it would be necessary to include the Arc as all of the implemented
    // fields are Arc'd anyway, and I dont think we need both
    inner: ArcSwapOption<Mutex<Box<T>>>,
}

impl<T: ?Sized> EventHandler<T> {
    pub fn empty() -> Self {
        Self {
            inner: ArcSwapOption::empty(),
        }
    }

    pub fn with_handler(handler: Box<T>) -> Self {
        Self {
            inner: Some(Arc::new(Mutex::new(handler))).into(),
        }
    }

    pub fn load(&self) -> Guard<Option<Arc<Mutex<Box<T>>>>> {
        //FIXME: if there was a way to get a MutexGuard<'_, T> instead of
        //having what we have now that would be great
        self.inner.load()
    }

    pub fn store(&self, handler: Box<T>) {
        self.inner.store(Some(Arc::new(Mutex::new(handler))))
    }

    pub fn swap(&mut self, handle: Box<T>) -> Option<Arc<Mutex<Box<T>>>> {
        self.inner.swap(Some(Arc::new(Mutex::new(handle))))
    }
}

impl<T: ?Sized> Default for EventHandler<T> {
    fn default() -> Self {
        Self::empty()
    }
}

mod test {
    use super::*;
    struct T {
        a: EventHandler<dyn Send + Sync>,
    }

    impl T {
        fn new(val: impl Send + Sync + 'static) -> Self {
            let a: EventHandler<dyn Send + Sync> = EventHandler::with_handler(Box::new(val));
            Self { a }
        }
    }
}
