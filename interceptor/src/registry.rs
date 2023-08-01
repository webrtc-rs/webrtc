use std::sync::Arc;

use crate::chain::Chain;
use crate::error::Result;
use crate::noop::NoOp;
use crate::{Interceptor, InterceptorBuilder};

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

    /// build constructs a single Interceptor from an InterceptorRegistry
    pub fn build(&self, id: &str) -> Result<Arc<dyn Interceptor + Send + Sync>> {
        if self.builders.is_empty() {
            return Ok(Arc::new(NoOp {}));
        }

        self.build_chain(id)
            .map(|c| Arc::new(c) as Arc<dyn Interceptor + Send + Sync>)
    }

    /// build_chain constructs a non-type erased Chain from an Interceptor registry.
    pub fn build_chain(&self, id: &str) -> Result<Chain> {
        if self.builders.is_empty() {
            return Ok(Chain::new(vec![Arc::new(NoOp {})]));
        }

        let interceptors: Result<Vec<_>> = self.builders.iter().map(|b| b.build(id)).collect();

        Ok(Chain::new(interceptors?))
    }
}
