use core::future::Future;
use core::pin::Pin;
use core::ptr::NonNull;
use core::task::{Context, Poll};

#[repr(transparent)]
pub struct FutureUnit<'a> {
    inner: NonNull<dyn Future<Output = ()> + Send + 'a>,
}

unsafe impl Send for FutureUnit<'_> {}

impl<'a> FutureUnit<'a> {
    pub fn from_async(async_fn: impl Future<Output = ()> + Send + 'a) -> Self {
        //FIXME: optimistically should be non-heap allocated as they should each contain only
        //one byte. I(jumbeldliam) would prefer to have a more ergonomic api upfront which can be
        //changed later (and as to why I made inner NonNull rather than Box)
        let boxed: Box<dyn Future<Output = ()> + Send + 'a> = Box::new(async_fn);
        let boxed = Box::into_raw(boxed);

        // SAFETY: Box::into_raw always returns a valid ptr
        let inner = unsafe { NonNull::new_unchecked(boxed) };
        FutureUnit { inner }
    }
}

impl Future for FutureUnit<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut inner = Pin::into_inner(self).inner;
        // SAFETY: the pin has not been moved so we have not moved out of the ptr
        let inner_pin = unsafe { Pin::new_unchecked(inner.as_mut()) };
        inner_pin.poll(cx)
    }
}

impl Unpin for FutureUnit<'_> {}

impl Drop for FutureUnit<'_> {
    fn drop(&mut self) {
        unsafe {
            // SAFETY: the pointer is still valid
            // so it is okay to construct a box out of it
            drop(Box::from_raw(self.inner.as_ptr()))
        }
    }
}

//TODO: proc macro for inline trait variants
