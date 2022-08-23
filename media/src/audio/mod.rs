pub mod buffer;
mod sample;

pub use sample::Sample;

mod sealed {
    pub trait Sealed {}
}
