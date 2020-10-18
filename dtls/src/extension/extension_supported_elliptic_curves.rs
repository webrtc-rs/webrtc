use super::*;
use crate::curve::named_curve::*;

const EXTENSION_SUPPORTED_GROUPS_HEADER_SIZE: usize = 6;

// https://tools.ietf.org/html/rfc8422#section-5.1.1
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionSupportedEllipticCurves {
    elliptic_curves: Vec<NamedCurve>,
}

impl ExtensionSupportedEllipticCurves {
    pub fn extension_value() -> ExtensionValue {
        ExtensionValue::SupportedEllipticCurves
    }
}
