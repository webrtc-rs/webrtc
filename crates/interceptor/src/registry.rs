use crate::chain::Chain;
use crate::error::Result;
use crate::noop::NoOp;
use crate::{Factory, Interceptor};

use std::sync::Arc;

/// Registry is a collector for interceptors.
#[derive(Default)]
pub struct Registry {
    factories: Vec<Box<dyn Factory + Send + Sync>>,
}

impl Registry {
    pub fn new() -> Self {
        Registry { factories: vec![] }
    }

    /// add adds a new Factory to the registry.
    pub fn add(&mut self, factory: Box<dyn Factory + Send + Sync>) {
        self.factories.push(factory);
    }

    /// build constructs a single Interceptor from a InterceptorRegistry
    pub fn build(&self, id: &str) -> Result<Arc<dyn Interceptor + Send + Sync>> {
        if self.factories.is_empty() {
            return Ok(Arc::new(NoOp {}));
        }

        let mut interceptors = vec![];
        for f in &self.factories {
            let icpr = f.new_interceptor(id)?;
            interceptors.push(icpr);
        }

        Ok(Arc::new(Chain::new(interceptors)))
    }
}
