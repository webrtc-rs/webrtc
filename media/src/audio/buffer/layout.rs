use crate::audio::buffer::BufferInfo;
use crate::audio::sealed::Sealed;

pub trait BufferLayout: Sized + Sealed {
    fn index_of(info: &BufferInfo<Self>, channel: usize, frame: usize) -> usize;
}

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum Deinterleaved {}

impl Sealed for Deinterleaved {}

impl BufferLayout for Deinterleaved {
    #[inline]
    fn index_of(info: &BufferInfo<Self>, channel: usize, frame: usize) -> usize {
        (channel * info.frames()) + frame
    }
}

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum Interleaved {}

impl Sealed for Interleaved {}

impl BufferLayout for Interleaved {
    #[inline]
    fn index_of(info: &BufferInfo<Self>, channel: usize, frame: usize) -> usize {
        (frame * info.channels()) + channel
    }
}

#[cfg(test)]
#[inline(always)]
pub(crate) fn deinterleaved<T>(input: &[T], output: &mut [T], channels: usize)
where
    T: Copy,
{
    deinterleaved_by(input, output, channels, |sample| *sample)
}

/// De-interleaves an interleaved slice using a memory access pattern
/// that's optimized for efficient cached (i.e. sequential) reads.
pub(crate) fn deinterleaved_by<T, U, F>(input: &[T], output: &mut [U], channels: usize, f: F)
where
    F: Fn(&T) -> U,
{
    assert_eq!(input.len(), output.len());
    assert_eq!(input.len() % channels, 0);

    let frames = input.len() / channels;
    let mut interleaved_index = 0;
    for frame in 0..frames {
        let mut deinterleaved_index = frame;
        for _channel in 0..channels {
            output[deinterleaved_index] = f(&input[interleaved_index]);
            interleaved_index += 1;
            deinterleaved_index += frames;
        }
    }
}

#[cfg(test)]
#[inline(always)]
pub(crate) fn interleaved<T>(input: &[T], output: &mut [T], channels: usize)
where
    T: Copy,
{
    interleaved_by(input, output, channels, |sample| *sample)
}

/// Interleaves an de-interleaved slice using a memory access pattern
/// that's optimized for efficient cached (i.e. sequential) reads.
pub(crate) fn interleaved_by<T, U, F>(input: &[T], output: &mut [U], channels: usize, f: F)
where
    F: Fn(&T) -> U,
{
    assert_eq!(input.len(), output.len());
    assert_eq!(input.len() % channels, 0);

    let frames = input.len() / channels;
    let mut deinterleaved_index = 0;
    for channel in 0..channels {
        let mut interleaved_index = channel;
        for _frame in 0..frames {
            output[interleaved_index] = f(&input[deinterleaved_index]);
            deinterleaved_index += 1;
            interleaved_index += channels;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interleaved_1_channel() {
        let input: Vec<_> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
        let mut output = vec![0; input.len()];
        let channels = 1;

        interleaved(&input[..], &mut output[..], channels);

        let actual = output;
        let expected = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

        assert_eq!(actual, expected);
    }

    #[test]
    fn deinterleaved_1_channel() {
        let input: Vec<_> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
        let mut output = vec![0; input.len()];
        let channels = 1;

        deinterleaved(&input[..], &mut output[..], channels);

        let actual = output;
        let expected = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

        assert_eq!(actual, expected);
    }

    #[test]
    fn interleaved_2_channel() {
        let input: Vec<_> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
        let mut output = vec![0; input.len()];
        let channels = 2;

        interleaved(&input[..], &mut output[..], channels);

        let actual = output;
        let expected = vec![0, 8, 1, 9, 2, 10, 3, 11, 4, 12, 5, 13, 6, 14, 7, 15];

        assert_eq!(actual, expected);
    }

    #[test]
    fn deinterleaved_2_channel() {
        let input: Vec<_> = vec![0, 8, 1, 9, 2, 10, 3, 11, 4, 12, 5, 13, 6, 14, 7, 15];
        let mut output = vec![0; input.len()];
        let channels = 2;

        deinterleaved(&input[..], &mut output[..], channels);

        let actual = output;
        let expected = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

        assert_eq!(actual, expected);
    }

    #[test]
    fn interleaved_3_channel() {
        let input: Vec<_> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];
        let mut output = vec![0; input.len()];
        let channels = 3;

        interleaved(&input[..], &mut output[..], channels);

        let actual = output;
        let expected = vec![0, 5, 10, 1, 6, 11, 2, 7, 12, 3, 8, 13, 4, 9, 14];

        assert_eq!(actual, expected);
    }

    #[test]
    fn deinterleaved_3_channel() {
        let input: Vec<_> = vec![0, 5, 10, 1, 6, 11, 2, 7, 12, 3, 8, 13, 4, 9, 14];
        let mut output = vec![0; input.len()];
        let channels = 3;

        deinterleaved(&input[..], &mut output[..], channels);

        let actual = output;
        let expected = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];

        assert_eq!(actual, expected);
    }
}
