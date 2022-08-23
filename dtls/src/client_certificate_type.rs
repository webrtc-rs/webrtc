#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ClientCertificateType {
    RsaSign = 1,
    EcdsaSign = 64,
    Unsupported,
}

impl From<u8> for ClientCertificateType {
    fn from(val: u8) -> Self {
        match val {
            1 => ClientCertificateType::RsaSign,
            64 => ClientCertificateType::EcdsaSign,
            _ => ClientCertificateType::Unsupported,
        }
    }
}
