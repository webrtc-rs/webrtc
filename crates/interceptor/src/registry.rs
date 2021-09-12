use super::*;

/// Registry is a collector for interceptors.
pub struct Registry {
    interceptors: Vec<Box<dyn Interceptor + Send + Sync>>,
}

impl Registry {
    /// add adds a new Interceptor to the registry.
    pub fn add(&mut self, icpr: Box<dyn Interceptor + Send + Sync>) {
        self.interceptors.push(icpr);
    }

    /*
    /// build constructs a single Interceptor from a InterceptorRegistry
    pub fn build(&self) -> Interceptor {
        if len(i.interceptors) == 0 {
            return &NoOp{}
        }

        return NewChain(i.interceptors)
    }*/
}
