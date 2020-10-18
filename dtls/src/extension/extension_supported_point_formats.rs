use super::*;

const EXTENSION_SUPPORTED_POINT_FORMATS_SIZE: usize = 5;

pub type EllipticCurvePointFormat = u8;

pub const ELLIPTIC_CURVE_POINT_FORMAT_UNCOMPRESSED: EllipticCurvePointFormat = 0;

// https://tools.ietf.org/html/rfc4492#section-5.1.2
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionSupportedPointFormats {
    point_formats: Vec<EllipticCurvePointFormat>,
}

impl ExtensionSupportedPointFormats {
    pub fn extension_value(&self) -> ExtensionValue {
        ExtensionValue::SupportedPointFormats
    }
}
