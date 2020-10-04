/*use hmac::{Hmac, Mac};
use sha1::Sha1;

use util::Error;

type HmacSha1 = Hmac<Sha1>;

pub(crate) struct CipherAesCmHmacSha1 {
    srtp_session_salt: Vec<u8>,
    srtp_session_auth: HmacSha1,
    srtp_session_auth_tag: Vec<u8>,

    srtcp_session_salt: Vec<u8>,
    srtcp_session_auth: HmacSha1,
    srtcp_session_auth_tag: Vec<u8>,
}

impl CipherAesCmHmacSha1 {
    pub fn new(master_key: Vec<u8>, master_salt: Vec<u8>) -> Result<Self, Error> {}
}
*/
