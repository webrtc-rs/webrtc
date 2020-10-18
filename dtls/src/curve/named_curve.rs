// https://www.iana.org/assignments/tls-parameters/tls-parameters.xml#tls-parameters-8
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum NamedCurve {
    P256 = 0x0017,
    P384 = 0x0018,
    X25519 = 0x001d,
    Unsupported,
}

impl From<u16> for NamedCurve {
    fn from(val: u16) -> Self {
        match val {
            0x0017 => NamedCurve::P256,
            0x0018 => NamedCurve::P384,
            0x001d => NamedCurve::X25519,
            _ => NamedCurve::Unsupported,
        }
    }
}

pub struct NamedCurveKeypair {
    curve: NamedCurve,
    public_key: Vec<u8>,
    private_key: Vec<u8>,
}
/*
fn elliptic_curve_keypair(curve: NamedCurve, c1:elliptic.Curve
                          c2: elliptic.Curve) ->Result<NamedCurveKeypair, Error> {
    private_key, x, y, err := elliptic.GenerateKey(c1, rand.Reader)
    if err != nil {
        return nil, err
    }

    Ok(NamedCurveKeypair{curve, elliptic.Marshal(c2, x, y), private_key})
}

impl NamedCurve{
    pub fn generate_keypair(&self) ->Result<NamedCurveKeypair, Error> {
        match *c {
         /*NamedCurve::X25519=>{
            tmp := make([]byte, 32)
            if _, err := rand.Read(tmp); err != nil {
                return nil, err
            }

            var public, private [32]byte
            copy(private[:], tmp)

            curve25519.ScalarBaseMult(&public, &private)
            Ok(NamedCurveKeypair{curve:NamedCurve::X25519, public_key, private_key})
        }*/
        NamedCurve::P256 => elliptic_curve_keypair(NamedCurve::P256, elliptic.P256(), elliptic.P256()),
        NamedCurve::P384 => elliptic_curve_keypair(NamedCurve::P384, elliptic.P384(), elliptic.P384()),
        _=> Err(ERR_INVALID_NAMED_CURVE.clone()),
    }
}*/
