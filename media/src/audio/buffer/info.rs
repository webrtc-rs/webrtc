use std::marker::PhantomData;

use crate::audio::buffer::layout::{Deinterleaved, Interleaved};

#[derive(Eq, PartialEq, Debug)]
pub struct BufferInfo<L> {
    channels: usize,
    frames: usize,
    _phantom: PhantomData<L>,
}

impl<L> BufferInfo<L> {
    pub fn new(channels: usize, frames: usize) -> Self {
        Self {
            channels,
            frames,
            _phantom: PhantomData,
        }
    }

    /// Get a reference to the buffer info's channels.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Set the buffer info's channels.
    pub fn set_channels(&mut self, channels: usize) {
        self.channels = channels;
    }

    /// Get a reference to the buffer info's frames.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Set the buffer info's frames.
    pub fn set_frames(&mut self, frames: usize) {
        self.frames = frames;
    }

    pub fn samples(&self) -> usize {
        self.channels * self.frames
    }
}

impl<L> Copy for BufferInfo<L> {}

impl<L> Clone for BufferInfo<L> {
    fn clone(&self) -> Self {
        *self
    }
}

macro_rules! impl_from_buffer_info {
    ($in_layout:ty => $out_layout:ty) => {
        impl From<BufferInfo<$in_layout>> for BufferInfo<$out_layout> {
            fn from(info: BufferInfo<$in_layout>) -> Self {
                Self {
                    channels: info.channels,
                    frames: info.frames,
                    _phantom: PhantomData,
                }
            }
        }
    };
}

impl_from_buffer_info!(Interleaved => Deinterleaved);
impl_from_buffer_info!(Deinterleaved => Interleaved);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new() {
        let channels = 3;
        let frames = 100;

        let interleaved = BufferInfo::<Interleaved>::new(channels, frames);

        assert_eq!(interleaved.channels, channels);
        assert_eq!(interleaved.frames, frames);

        let deinterleaved = BufferInfo::<Deinterleaved>::new(channels, frames);

        assert_eq!(deinterleaved.channels, channels);
        assert_eq!(deinterleaved.frames, frames);
    }

    #[test]
    fn clone() {
        let channels = 3;
        let frames = 100;

        let interleaved = BufferInfo::<Interleaved>::new(channels, frames);

        assert_eq!(interleaved.clone(), interleaved);

        let deinterleaved = BufferInfo::<Deinterleaved>::new(channels, frames);

        assert_eq!(deinterleaved.clone(), deinterleaved);
    }

    #[test]
    fn samples() {
        let channels = 3;
        let frames = 100;

        let interleaved = BufferInfo::<Interleaved>::new(channels, frames);

        assert_eq!(interleaved.samples(), channels * frames);

        let deinterleaved = BufferInfo::<Deinterleaved>::new(channels, frames);

        assert_eq!(deinterleaved.samples(), channels * frames);
    }
}
