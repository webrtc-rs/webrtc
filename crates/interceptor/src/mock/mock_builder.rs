use crate::error::Result;
use crate::{Interceptor, InterceptorBuilder};
use std::sync::Arc;

/// MockBuilder is a mock Builder for testing.
pub struct MockBuilder<'a> {
    pub build: Box<dyn 'a + (Fn(&str) -> Result<Arc<dyn Interceptor + Send + Sync>>)>,
}

impl<'a> MockBuilder<'a> {
    pub fn new<F: 'a + (Fn(&str) -> Result<Arc<dyn Interceptor + Send + Sync>>)>(f: F) -> Self {
        MockBuilder { build: Box::new(f) }
    }
}

impl<'a> InterceptorBuilder for MockBuilder<'a> {
    fn build(&self, id: &str) -> Result<Arc<dyn Interceptor + Send + Sync>> {
        (self.build)(id)
    }
}
