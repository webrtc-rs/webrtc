/// A synchronous mutual exclusion primitive useful for protecting shared data
pub type Mutex<T> = parking_lot::Mutex<T>;

/// A synchronous reader-writer lock
pub type RwLock<T> = parking_lot::RwLock<T>;

/// A synchronization primitive which can be used to run a one-time initialization.
pub type Once = parking_lot::Once;
