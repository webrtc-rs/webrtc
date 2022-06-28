pub mod named_curve;

// https://www.iana.org/assignments/tls-parameters/tls-parameters.xhtml#tls-parameters-10
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EllipticCurveType {
    NamedCurve = 0x03,
    Unsupported,
}

impl From<u8> for EllipticCurveType {
    fn from(val: u8) -> Self {
        match val {
            0x03 => EllipticCurveType::NamedCurve,
            _ => EllipticCurveType::Unsupported,
        }
    }
}
