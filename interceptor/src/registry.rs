use crate::chain::Chain;
use crate::error::Result;
use crate::noop::NoOp;
use crate::{Interceptor, InterceptorBuilder};

use std::sync::Arc;

/// Registry is a collector for interceptors.
#[derive(Default)]
pub struct Registry {
    builders: Vec<Box<dyn InterceptorBuilder + Send + Sync>>,
}

impl Registry {
    pub fn new() -> Self {
        Registry { builders: vec![] }
    }

    /// add adds a new InterceptorBuilder to the registry.
    pub fn add(&mut self, builder: Box<dyn InterceptorBuilder + Send + Sync>) {
        self.builders.push(builder);
    }

    /// build constructs a single Interceptor from a InterceptorRegistry
    pub fn build(&self, id: &str) -> Result<Arc<dyn Interceptor + Send + Sync>> {
        if self.builders.is_empty() {
            return Ok(Arc::new(NoOp {}));
        }

        let mut interceptors = vec![];
        for f in &self.builders {
            let icpr = f.build(id)?;
            interceptors.push(icpr);
        }

        Ok(Arc::new(Chain::new(interceptors)))
    }
}
