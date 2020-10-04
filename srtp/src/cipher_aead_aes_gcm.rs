const RTCP_ENCRYPTION_FLAG: u8 = 0x80;

pub(crate) struct CipherAeadAesGcm {
    //srtpCipher, srtcpCipher cipher.AEAD
//srtpSessionSalt, srtcpSessionSalt []byte
}

impl CipherAeadAesGcm {
    pub(crate) fn auth_tag_len() -> usize {
        16
    }
}
