use crate::error::Result;
use crate::{Factory, Interceptor};
use std::sync::Arc;

/// MockFactory is a mock Factory for testing.
pub struct MockFactory {
    new_interceptor_fn: Box<dyn Fn(&str) -> Result<Arc<dyn Interceptor + Send + Sync>>>,
}

impl Factory for MockFactory {
    /// new_interceptor implements Interceptor
    fn new_interceptor(&self, id: &str) -> Result<Arc<dyn Interceptor + Send + Sync>> {
        (self.new_interceptor_fn)(id)
    }
}
