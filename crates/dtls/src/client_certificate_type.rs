#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ClientCertificateType {
    RSASign = 1,
    ECDSASign = 64,
    Unsupported,
}

impl From<u8> for ClientCertificateType {
    fn from(val: u8) -> Self {
        match val {
            1 => ClientCertificateType::RSASign,
            64 => ClientCertificateType::ECDSASign,
            _ => ClientCertificateType::Unsupported,
        }
    }
}
