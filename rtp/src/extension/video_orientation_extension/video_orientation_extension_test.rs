use bytes::{Bytes, BytesMut};

use super::*;
use crate::error::Result;

#[test]
fn test_video_orientation_extension_too_small() -> Result<()> {
    let mut buf = &vec![0u8; 0][..];
    let result = VideoOrientationExtension::unmarshal(&mut buf);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn test_video_orientation_extension_back_facing_camera() -> Result<()> {
    let raw = Bytes::from_static(&[0b1000]);
    let buf = &mut raw.clone();
    let a1 = VideoOrientationExtension::unmarshal(buf)?;
    let a2 = VideoOrientationExtension {
        direction: CameraDirection::Back,
        flip: false,
        rotation: VideoRotation::Degree0,
    };
    assert_eq!(a1, a2);

    let mut dst = BytesMut::with_capacity(a2.marshal_size());
    dst.resize(a2.marshal_size(), 0);
    a2.marshal_to(&mut dst)?;
    assert_eq!(raw, dst.freeze());

    Ok(())
}

#[test]
fn test_video_orientation_extension_flip_true() -> Result<()> {
    let raw = Bytes::from_static(&[0b0100]);
    let buf = &mut raw.clone();
    let a1 = VideoOrientationExtension::unmarshal(buf)?;
    let a2 = VideoOrientationExtension {
        direction: CameraDirection::Front,
        flip: true,
        rotation: VideoRotation::Degree0,
    };
    assert_eq!(a1, a2);

    let mut dst = BytesMut::with_capacity(a2.marshal_size());
    dst.resize(a2.marshal_size(), 0);
    a2.marshal_to(&mut dst)?;
    assert_eq!(raw, dst.freeze());

    Ok(())
}

#[test]
fn test_video_orientation_extension_degree_90() -> Result<()> {
    let raw = Bytes::from_static(&[0b0001]);
    let buf = &mut raw.clone();
    let a1 = VideoOrientationExtension::unmarshal(buf)?;
    let a2 = VideoOrientationExtension {
        direction: CameraDirection::Front,
        flip: false,
        rotation: VideoRotation::Degree90,
    };
    assert_eq!(a1, a2);

    let mut dst = BytesMut::with_capacity(a2.marshal_size());
    dst.resize(a2.marshal_size(), 0);
    a2.marshal_to(&mut dst)?;
    assert_eq!(raw, dst.freeze());

    Ok(())
}

#[test]
fn test_video_orientation_extension_degree_180() -> Result<()> {
    let raw = Bytes::from_static(&[0b0010]);
    let buf = &mut raw.clone();
    let a1 = VideoOrientationExtension::unmarshal(buf)?;
    let a2 = VideoOrientationExtension {
        direction: CameraDirection::Front,
        flip: false,
        rotation: VideoRotation::Degree180,
    };
    assert_eq!(a1, a2);

    let mut dst = BytesMut::with_capacity(a2.marshal_size());
    dst.resize(a2.marshal_size(), 0);
    a2.marshal_to(&mut dst)?;
    assert_eq!(raw, dst.freeze());

    Ok(())
}

#[test]
fn test_video_orientation_extension_degree_270() -> Result<()> {
    let raw = Bytes::from_static(&[0b0011]);
    let buf = &mut raw.clone();
    let a1 = VideoOrientationExtension::unmarshal(buf)?;
    let a2 = VideoOrientationExtension {
        direction: CameraDirection::Front,
        flip: false,
        rotation: VideoRotation::Degree270,
    };
    assert_eq!(a1, a2);

    let mut dst = BytesMut::with_capacity(a2.marshal_size());
    dst.resize(a2.marshal_size(), 0);
    a2.marshal_to(&mut dst)?;
    assert_eq!(raw, dst.freeze());

    Ok(())
}
