use std::io::{Cursor, Read};

use byteorder::{ByteOrder, ReadBytesExt};
#[cfg(test)]
use nearly_eq::NearlyEq;

#[derive(Eq, PartialEq, Copy, Clone, Default, Debug)]
#[repr(transparent)]
pub struct Sample<Raw>(Raw);

impl From<i16> for Sample<i16> {
    #[inline]
    fn from(raw: i16) -> Self {
        Self(raw)
    }
}

impl From<f32> for Sample<f32> {
    #[inline]
    fn from(raw: f32) -> Self {
        Self(raw.clamp(-1.0, 1.0))
    }
}

macro_rules! impl_from_sample_for_raw {
    ($raw:ty) => {
        impl From<Sample<$raw>> for $raw {
            #[inline]
            fn from(sample: Sample<$raw>) -> $raw {
                sample.0
            }
        }
    };
}

impl_from_sample_for_raw!(i16);
impl_from_sample_for_raw!(f32);

// impl From<Sample<i16>> for Sample<i64> {
//     #[inline]
//     fn from(sample: Sample<i16>) -> Self {
//         // Fast but imprecise approach:
//         // Perform crude but fast upsample by bit-shifting the raw value:
//         Self::from((sample.0 as i64) << 16)

//         // Slow but precise approach:
//         // Perform a proper but expensive lerp from
//         // i16::MIN..i16::MAX to i32::MIN..i32::MAX:

//         // let value = sample.0 as i64;

//         // let from = if value <= 0 { i16::MIN } else { i16::MAX } as i64;
//         // let to = if value <= 0 { i32::MIN } else { i32::MAX } as i64;

//         // Self::from((value * to + from / 2) / from)
//     }
// }

impl From<Sample<i16>> for Sample<f32> {
    #[inline]
    fn from(sample: Sample<i16>) -> Self {
        let divisor = if sample.0 < 0 {
            i16::MIN as f32
        } else {
            i16::MAX as f32
        }
        .abs();
        Self::from((sample.0 as f32) / divisor)
    }
}

impl From<Sample<f32>> for Sample<i16> {
    #[inline]
    fn from(sample: Sample<f32>) -> Self {
        let multiplier = if sample.0 < 0.0 {
            i16::MIN as f32
        } else {
            i16::MAX as f32
        }
        .abs();
        Self::from((sample.0 * multiplier) as i16)
    }
}

trait FromBytes: Sized {
    fn from_reader<B: ByteOrder, R: Read>(reader: &mut R) -> Result<Self, std::io::Error>;

    fn from_bytes<B: ByteOrder>(bytes: &[u8]) -> Result<Self, std::io::Error> {
        let mut cursor = Cursor::new(bytes);
        Self::from_reader::<B, _>(&mut cursor)
    }
}

impl FromBytes for Sample<i16> {
    fn from_reader<B: ByteOrder, R: Read>(reader: &mut R) -> Result<Self, std::io::Error> {
        reader.read_i16::<B>().map(Self::from)
    }
}

impl FromBytes for Sample<f32> {
    fn from_reader<B: ByteOrder, R: Read>(reader: &mut R) -> Result<Self, std::io::Error> {
        reader.read_f32::<B>().map(Self::from)
    }
}

#[cfg(test)]
impl<Raw> NearlyEq<Self, Raw> for Sample<Raw>
where
    Raw: NearlyEq<Raw, Raw>,
{
    fn eps() -> Raw {
        Raw::eps()
    }

    fn eq(&self, other: &Self, eps: &Raw) -> bool {
        NearlyEq::eq(&self.0, &other.0, eps)
    }
}

#[cfg(test)]
mod tests {
    use nearly_eq::assert_nearly_eq;

    use super::*;

    #[test]
    fn sample_i16_from_i16() {
        // i16:
        assert_eq!(Sample::<i16>::from(i16::MIN).0, i16::MIN);
        assert_eq!(Sample::<i16>::from(i16::MIN / 2).0, i16::MIN / 2);
        assert_eq!(Sample::<i16>::from(0).0, 0);
        assert_eq!(Sample::<i16>::from(i16::MAX / 2).0, i16::MAX / 2);
        assert_eq!(Sample::<i16>::from(i16::MAX).0, i16::MAX);
    }

    #[test]
    fn sample_f32_from_f32() {
        assert_eq!(Sample::<f32>::from(-1.0).0, -1.0);
        assert_eq!(Sample::<f32>::from(-0.5).0, -0.5);
        assert_eq!(Sample::<f32>::from(0.0).0, 0.0);
        assert_eq!(Sample::<f32>::from(0.5).0, 0.5);
        assert_eq!(Sample::<f32>::from(1.0).0, 1.0);

        // For any values outside of -1.0..=1.0 we expect clamping:
        assert_eq!(Sample::<f32>::from(f32::MIN).0, -1.0);
        assert_eq!(Sample::<f32>::from(f32::MAX).0, 1.0);
    }

    #[test]
    fn sample_i16_from_sample_f32() {
        assert_nearly_eq!(
            Sample::<i16>::from(Sample::<f32>::from(-1.0)),
            Sample::from(i16::MIN)
        );
        assert_nearly_eq!(
            Sample::<i16>::from(Sample::<f32>::from(-0.5)),
            Sample::from(i16::MIN / 2)
        );
        assert_nearly_eq!(
            Sample::<i16>::from(Sample::<f32>::from(0.0)),
            Sample::from(0)
        );
        assert_nearly_eq!(
            Sample::<i16>::from(Sample::<f32>::from(0.5)),
            Sample::from(i16::MAX / 2)
        );
        assert_nearly_eq!(
            Sample::<i16>::from(Sample::<f32>::from(1.0)),
            Sample::from(i16::MAX)
        );
    }

    #[test]
    fn sample_f32_from_sample_i16() {
        assert_nearly_eq!(
            Sample::<f32>::from(Sample::<i16>::from(i16::MIN)),
            Sample::from(-1.0)
        );
        assert_nearly_eq!(
            Sample::<f32>::from(Sample::<i16>::from(i16::MIN / 2)),
            Sample::from(-0.5)
        );
        assert_nearly_eq!(
            Sample::<f32>::from(Sample::<i16>::from(0)),
            Sample::from(0.0)
        );
        assert_nearly_eq!(
            Sample::<f32>::from(Sample::<i16>::from(i16::MAX / 2)),
            Sample::from(0.5),
            0.0001 // rounding error due to i16::MAX being odd
        );
        assert_nearly_eq!(
            Sample::<f32>::from(Sample::<i16>::from(i16::MAX)),
            Sample::from(1.0)
        );
    }
}
