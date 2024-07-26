pub mod info;
pub mod layout;

use std::mem::{ManuallyDrop, MaybeUninit};
use std::ops::Range;

use byteorder::ByteOrder;
pub use info::BufferInfo;
pub use layout::BufferLayout;
use layout::{Deinterleaved, Interleaved};
use thiserror::Error;

pub trait FromBytes<L>: Sized {
    type Error;

    fn from_bytes<B: ByteOrder>(bytes: &[u8], channels: usize) -> Result<Self, Self::Error>;
}

pub trait ToByteBufferRef<L>: Sized {
    type Error;

    fn bytes_len(&self);
    fn to_bytes<B: ByteOrder>(
        &self,
        bytes: &mut [u8],
        channels: usize,
    ) -> Result<usize, Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("Unexpected end of buffer: (expected: {expected}, actual: {actual})")]
    UnexpectedEndOfBuffer { expected: usize, actual: usize },
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct BufferRef<'a, T, L> {
    samples: &'a [T],
    info: BufferInfo<L>,
}

impl<'a, T, L> BufferRef<'a, T, L> {
    pub fn new(samples: &'a [T], channels: usize) -> Self {
        debug_assert_eq!(samples.len() % channels, 0);
        let info = {
            let frames = samples.len() / channels;
            BufferInfo::new(channels, frames)
        };
        Self { samples, info }
    }
}

/// Buffer multi-channel interlaced Audio.
#[derive(Eq, PartialEq, Clone, Debug)]
pub struct Buffer<T, L> {
    samples: Vec<T>,
    info: BufferInfo<L>,
}

impl<T, L> Buffer<T, L> {
    pub fn new(samples: Vec<T>, channels: usize) -> Self {
        debug_assert_eq!(samples.len() % channels, 0);
        let info = {
            let frames = samples.len() / channels;
            BufferInfo::new(channels, frames)
        };
        Self { samples, info }
    }

    pub fn as_ref(&'_ self) -> BufferRef<'_, T, L> {
        BufferRef {
            samples: &self.samples[..],
            info: self.info,
        }
    }

    pub fn sub_range(&'_ self, range: Range<usize>) -> BufferRef<'_, T, L> {
        let samples_len = range.len();
        let samples = &self.samples[range];
        let info = {
            let channels = self.info.channels();
            assert_eq!(samples_len % channels, 0);
            let frames = samples_len / channels;
            BufferInfo::new(channels, frames)
        };
        BufferRef { samples, info }
    }
}

impl<T> From<Buffer<T, Deinterleaved>> for Buffer<T, Interleaved>
where
    T: Default + Copy,
{
    fn from(buffer: Buffer<T, Deinterleaved>) -> Self {
        Self::from(buffer.as_ref())
    }
}

impl<'a, T> From<BufferRef<'a, T, Deinterleaved>> for Buffer<T, Interleaved>
where
    T: Default + Copy,
{
    fn from(buffer: BufferRef<'a, T, Deinterleaved>) -> Self {
        // Writing into a vec of uninitialized `samples` is about 10% faster than
        // cloning it or creating a default-initialized one and over-writing it.
        //
        // # Safety
        //
        // The performance boost comes with a cost though:
        // At the end of the block each and every single item in
        // `samples` needs to have been initialized, or else you get UB!
        let samples = {
            // Create a vec of uninitialized samples.
            let mut samples: Vec<MaybeUninit<T>> =
                vec![MaybeUninit::uninit(); buffer.samples.len()];

            // Initialize all of its values:
            layout::interleaved_by(
                buffer.samples,
                &mut samples[..],
                buffer.info.channels(),
                |sample| MaybeUninit::new(*sample),
            );

            // Transmute the vec to the initialized type.
            unsafe { std::mem::transmute::<Vec<MaybeUninit<T>>, Vec<T>>(samples) }
        };

        let info = buffer.info.into();
        Self { samples, info }
    }
}

impl<T> From<Buffer<T, Interleaved>> for Buffer<T, Deinterleaved>
where
    T: Default + Copy,
{
    fn from(buffer: Buffer<T, Interleaved>) -> Self {
        Self::from(buffer.as_ref())
    }
}

impl<'a, T> From<BufferRef<'a, T, Interleaved>> for Buffer<T, Deinterleaved>
where
    T: Default + Copy,
{
    fn from(buffer: BufferRef<'a, T, Interleaved>) -> Self {
        // Writing into a vec of uninitialized `samples` is about 10% faster than
        // cloning it or creating a default-initialized one and over-writing it.
        //
        // # Safety
        //
        // The performance boost comes with a cost though:
        // At the end of the block each and every single item in
        // `samples` needs to have been initialized, or else you get UB!
        let samples = {
            // Create a vec of uninitialized samples.
            let mut samples: Vec<MaybeUninit<T>> =
                vec![MaybeUninit::uninit(); buffer.samples.len()];

            // Initialize the vec's values:
            layout::deinterleaved_by(
                buffer.samples,
                &mut samples[..],
                buffer.info.channels(),
                |sample| MaybeUninit::new(*sample),
            );

            // Everything is initialized. Transmute the vec to the initialized type.
            unsafe { std::mem::transmute::<Vec<MaybeUninit<T>>, Vec<T>>(samples) }
        };

        let info = buffer.info.into();
        Self { samples, info }
    }
}

impl FromBytes<Interleaved> for Buffer<i16, Interleaved> {
    type Error = ();

    fn from_bytes<B: ByteOrder>(bytes: &[u8], channels: usize) -> Result<Self, Self::Error> {
        const STRIDE: usize = std::mem::size_of::<i16>();
        assert_eq!(bytes.len() % STRIDE, 0);

        let chunks = {
            let chunks_ptr = bytes.as_ptr() as *const [u8; STRIDE];
            let chunks_len = bytes.len() / STRIDE;
            unsafe { std::slice::from_raw_parts(chunks_ptr, chunks_len) }
        };

        let samples: Vec<_> = chunks.iter().map(|chunk| B::read_i16(&chunk[..])).collect();

        let info = {
            let frames = samples.len() / channels;
            BufferInfo::new(channels, frames)
        };
        Ok(Self { samples, info })
    }
}

impl FromBytes<Deinterleaved> for Buffer<i16, Interleaved> {
    type Error = ();

    fn from_bytes<B: ByteOrder>(bytes: &[u8], channels: usize) -> Result<Self, Self::Error> {
        const STRIDE: usize = std::mem::size_of::<i16>();
        assert_eq!(bytes.len() % STRIDE, 0);

        let chunks = {
            let chunks_ptr = bytes.as_ptr() as *const [u8; STRIDE];
            let chunks_len = bytes.len() / STRIDE;
            unsafe { std::slice::from_raw_parts(chunks_ptr, chunks_len) }
        };

        // Writing into a vec of uninitialized `samples` is about 10% faster than
        // cloning it or creating a default-initialized one and over-writing it.
        //
        // # Safety
        //
        // The performance boost comes with a cost though:
        // At the end of the block each and every single item in
        // `samples` needs to have been initialized, or else you get UB!
        let samples = unsafe {
            init_vec(chunks.len(), |samples| {
                layout::interleaved_by(chunks, samples, channels, |chunk| {
                    MaybeUninit::new(B::read_i16(&chunk[..]))
                });
            })
        };

        let info = {
            let frames = samples.len() / channels;
            BufferInfo::new(channels, frames)
        };
        Ok(Self { samples, info })
    }
}

impl FromBytes<Deinterleaved> for Buffer<i16, Deinterleaved> {
    type Error = ();

    fn from_bytes<B: ByteOrder>(bytes: &[u8], channels: usize) -> Result<Self, Self::Error> {
        const STRIDE: usize = std::mem::size_of::<i16>();
        assert_eq!(bytes.len() % STRIDE, 0);

        let chunks = {
            let chunks_ptr = bytes.as_ptr() as *const [u8; STRIDE];
            let chunks_len = bytes.len() / STRIDE;
            unsafe { std::slice::from_raw_parts(chunks_ptr, chunks_len) }
        };

        let samples: Vec<_> = chunks.iter().map(|chunk| B::read_i16(&chunk[..])).collect();

        let info = {
            let frames = samples.len() / channels;
            BufferInfo::new(channels, frames)
        };
        Ok(Self { samples, info })
    }
}

impl FromBytes<Interleaved> for Buffer<i16, Deinterleaved> {
    type Error = ();

    fn from_bytes<B: ByteOrder>(bytes: &[u8], channels: usize) -> Result<Self, Self::Error> {
        const STRIDE: usize = std::mem::size_of::<i16>();
        assert_eq!(bytes.len() % STRIDE, 0);

        let chunks = {
            let chunks_ptr = bytes.as_ptr() as *const [u8; STRIDE];
            let chunks_len = bytes.len() / STRIDE;
            unsafe { std::slice::from_raw_parts(chunks_ptr, chunks_len) }
        };

        // Writing into a vec of uninitialized `samples` is about 10% faster than
        // cloning it or creating a default-initialized one and over-writing it.
        //
        // # Safety
        //
        // The performance boost comes with a cost though:
        // At the end of the block each and every single item in
        // `samples` needs to have been initialized, or else you get UB!
        let samples = unsafe {
            init_vec(chunks.len(), |samples| {
                layout::deinterleaved_by(chunks, samples, channels, |chunk| {
                    MaybeUninit::new(B::read_i16(&chunk[..]))
                });
            })
        };

        let info = {
            let frames = samples.len() / channels;
            BufferInfo::new(channels, frames)
        };
        Ok(Self { samples, info })
    }
}

/// Creates a vec with deferred initialization.
///
/// # Safety
///
/// The closure `f` MUST initialize every single item in the provided slice.
unsafe fn init_vec<T, F>(len: usize, f: F) -> Vec<T>
where
    MaybeUninit<T>: Clone,
    F: FnOnce(&mut [MaybeUninit<T>]),
{
    // Create a vec of uninitialized values.
    let mut vec: Vec<MaybeUninit<T>> = vec![MaybeUninit::uninit(); len];

    // Initialize values:
    f(&mut vec[..]);

    // Take owner-ship away from `vec`:
    let mut manually_drop: ManuallyDrop<_> = ManuallyDrop::new(vec);

    // Create vec of proper type from `vec`'s raw parts.
    let ptr = manually_drop.as_mut_ptr() as *mut T;
    let len = manually_drop.len();
    let cap = manually_drop.capacity();
    Vec::from_raw_parts(ptr, len, cap)
}

#[cfg(test)]
mod tests {
    use byteorder::NativeEndian;

    use super::*;

    #[test]
    fn deinterleaved_from_interleaved() {
        let channels = 3;

        let input_samples: Vec<i32> = vec![0, 5, 10, 1, 6, 11, 2, 7, 12, 3, 8, 13, 4, 9, 14];
        let input: Buffer<i32, Interleaved> = Buffer::new(input_samples, channels);

        let output = Buffer::<i32, Deinterleaved>::from(input);

        let actual = output.samples;
        let expected = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];

        assert_eq!(actual, expected);
    }

    #[test]
    fn interleaved_from_deinterleaved() {
        let channels = 3;

        let input_samples: Vec<i32> = vec![0, 3, 6, 9, 12, 1, 4, 7, 10, 13, 2, 5, 8, 11, 14];
        let input: Buffer<i32, Deinterleaved> = Buffer::new(input_samples, channels);

        let output = Buffer::<i32, Interleaved>::from(input);

        let actual = output.samples;
        let expected = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];

        assert_eq!(actual, expected);
    }

    #[test]
    fn deinterleaved_from_deinterleaved_bytes() {
        let channels = 3;
        let stride = 2;

        let input_samples: Vec<i16> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];
        let input_bytes: &[u8] = {
            let bytes_ptr = input_samples.as_ptr() as *const u8;
            let bytes_len = input_samples.len() * stride;
            unsafe { std::slice::from_raw_parts(bytes_ptr, bytes_len) }
        };

        let output: Buffer<i16, Deinterleaved> =
            FromBytes::<Deinterleaved>::from_bytes::<NativeEndian>(input_bytes, channels).unwrap();

        let actual = output.samples;
        let expected = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];

        assert_eq!(actual, expected);
    }

    #[test]
    fn deinterleaved_from_interleaved_bytes() {
        let channels = 3;
        let stride = 2;

        let input_samples: Vec<i16> = vec![0, 5, 10, 1, 6, 11, 2, 7, 12, 3, 8, 13, 4, 9, 14];
        let input_bytes: &[u8] = {
            let bytes_ptr = input_samples.as_ptr() as *const u8;
            let bytes_len = input_samples.len() * stride;
            unsafe { std::slice::from_raw_parts(bytes_ptr, bytes_len) }
        };

        let output: Buffer<i16, Deinterleaved> =
            FromBytes::<Interleaved>::from_bytes::<NativeEndian>(input_bytes, channels).unwrap();

        let actual = output.samples;
        let expected = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];

        assert_eq!(actual, expected);
    }

    #[test]
    fn interleaved_from_interleaved_bytes() {
        let channels = 3;
        let stride = 2;

        let input_samples: Vec<i16> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];
        let input_bytes: &[u8] = {
            let bytes_ptr = input_samples.as_ptr() as *const u8;
            let bytes_len = input_samples.len() * stride;
            unsafe { std::slice::from_raw_parts(bytes_ptr, bytes_len) }
        };

        let output: Buffer<i16, Interleaved> =
            FromBytes::<Interleaved>::from_bytes::<NativeEndian>(input_bytes, channels).unwrap();

        let actual = output.samples;
        let expected = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];

        assert_eq!(actual, expected);
    }

    #[test]
    fn interleaved_from_deinterleaved_bytes() {
        let channels = 3;
        let stride = 2;

        let input_samples: Vec<i16> = vec![0, 3, 6, 9, 12, 1, 4, 7, 10, 13, 2, 5, 8, 11, 14];
        let input_bytes: &[u8] = {
            let bytes_ptr = input_samples.as_ptr() as *const u8;
            let bytes_len = input_samples.len() * stride;
            unsafe { std::slice::from_raw_parts(bytes_ptr, bytes_len) }
        };

        let output: Buffer<i16, Interleaved> =
            FromBytes::<Deinterleaved>::from_bytes::<NativeEndian>(input_bytes, channels).unwrap();

        let actual = output.samples;
        let expected = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];

        assert_eq!(actual, expected);
    }
}
