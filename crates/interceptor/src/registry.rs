use crate::chain::Chain;
use crate::noop::NoOp;
use crate::Interceptor;

use std::sync::Arc;

/// Registry is a collector for interceptors.
#[derive(Default)]
pub struct Registry {
    interceptors: Vec<Arc<dyn Interceptor + Send + Sync>>,
}

impl Registry {
    pub fn new() -> Self {
        Registry {
            interceptors: vec![],
        }
    }

    /// with_interceptor adds a new Interceptor to the registry.
    pub fn with_interceptor(mut self, icpr: Arc<dyn Interceptor + Send + Sync>) -> Self {
        self.interceptors.push(icpr);
        self
    }

    /// build constructs a single Interceptor from a InterceptorRegistry
    pub fn build(mut self) -> Arc<dyn Interceptor + Send + Sync> {
        if self.interceptors.is_empty() {
            return Arc::new(NoOp {});
        }

        let interceptors: Vec<Arc<dyn Interceptor + Send + Sync>> =
            self.interceptors.drain(..).collect();

        Arc::new(Chain::new(interceptors))
    }
}
