mod extension_server_name;
mod extension_supported_elliptic_curves;
mod extension_supported_point_formats;
mod extension_supported_signature_algorithms;
mod extension_use_extended_master_secret;
mod extension_use_srtp;

use extension_server_name::*;
use extension_supported_elliptic_curves::*;
use extension_supported_point_formats::*;
use extension_supported_signature_algorithms::*;
use extension_use_extended_master_secret::*;
use extension_use_srtp::*;

// https://www.iana.org/assignments/tls-extensiontype-values/tls-extensiontype-values.xhtml
#[derive(Clone, Debug, PartialEq)]
pub enum ExtensionValue {
    ServerName = 0,
    SupportedEllipticCurves = 10,
    SupportedPointFormats = 11,
    SupportedSignatureAlgorithms = 13,
    UseSRTP = 14,
    UseExtendedMasterSecret = 23,
}

pub enum Extension {
    ServerName(ExtensionServerName),
    SupportedEllipticCurves(ExtensionSupportedEllipticCurves),
    SupportedPointFormats(ExtensionSupportedPointFormats),
    SupportedSignatureAlgorithms(ExtensionSupportedSignatureAlgorithms),
    UseSRTP(ExtensionUseSRTP),
    UseExtendedMasterSecret(ExtensionUseExtendedMasterSecret),
}

/*
func decodeExtensions(buf []byte) ([]extension, error) {
    if len(buf) < 2 {
        return nil, errBufferTooSmall
    }
    declaredLen := binary.BigEndian.Uint16(buf)
    if len(buf)-2 != int(declaredLen) {
        return nil, errLengthMismatch
    }

    extensions := []extension{}
    unmarshalAndAppend := func(data []byte, e extension) error {
        err := e.Unmarshal(data)
        if err != nil {
            return err
        }
        extensions = append(extensions, e)
        return nil
    }

    for offset := 2; offset < len(buf); {
        if len(buf) < (offset + 2) {
            return nil, errBufferTooSmall
        }
        var err error
        switch extension_value(binary.BigEndian.Uint16(buf[offset:])) {
        case extensionServerNameValue:
            err = unmarshalAndAppend(buf[offset:], &extensionServerName{})
        case extensionSupportedEllipticCurvesValue:
            err = unmarshalAndAppend(buf[offset:], &ExtensionSupportedEllipticCurves{})
        case extensionUseSRTPValue:
            err = unmarshalAndAppend(buf[offset:], &ExtensionUseSrtp{})
        case extensionUseExtendedMasterSecretValue:
            err = unmarshalAndAppend(buf[offset:], &ExtensionUseExtendedMasterSecret{})
        default:
        }
        if err != nil {
            return nil, err
        }
        if len(buf) < (offset + 4) {
            return nil, errBufferTooSmall
        }
        extensionLength := binary.BigEndian.Uint16(buf[offset+2:])
        offset += (4 + int(extensionLength))
    }
    return extensions, nil
}

func encodeExtensions(e []extension) ([]byte, error) {
    extensions := []byte{}
    for _, e := range e {
        raw, err := e.Marshal()
        if err != nil {
            return nil, err
        }
        extensions = append(extensions, raw...)
    }
    out := []byte{0x00, 0x00}
    binary.BigEndian.PutUint16(out, uint16(len(extensions)))
    return append(out, extensions...), nil
}
*/
