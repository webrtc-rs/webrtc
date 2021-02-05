#[cfg(test)]
mod rand_test;

use rand::{thread_rng, Rng};

const RUNES_ALPHA: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
const RUNES_CANDIDATE_ID_FOUNDATION: &[u8] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789/";

const LEN_UFRAG: usize = 16;
const LEN_PWD: usize = 32;

pub(crate) fn generate_cand_id() -> String {
    // https://tools.ietf.org/html/rfc5245#section-15.1
    // candidate-id = "candidate" ":" foundation
    // foundation   = 1*32ice-char
    // ice-char     = ALPHA / DIGIT / "+" / "/"
    let mut rng = thread_rng();

    let rand_string: String = (0..32)
        .map(|_| {
            let idx = rng.gen_range(0..RUNES_CANDIDATE_ID_FOUNDATION.len());
            RUNES_CANDIDATE_ID_FOUNDATION[idx] as char
        })
        .collect();

    format!("candidate:{}", rand_string)
}

// generate_pwd generates ICE pwd.
// This internally uses generateCryptoRandomString.
pub(crate) fn generate_pwd() -> String {
    let mut rng = thread_rng();

    let rand_pwd: String = (0..LEN_PWD)
        .map(|_| {
            let idx = rng.gen_range(0..RUNES_ALPHA.len());
            RUNES_ALPHA[idx] as char
        })
        .collect();

    rand_pwd
}

// generate_ufrag generates ICE user fragment.
// This internally uses generateCryptoRandomString.
pub(crate) fn generate_ufrag() -> String {
    let mut rng = thread_rng();

    let rand_ufrag: String = (0..LEN_UFRAG)
        .map(|_| {
            let idx = rng.gen_range(0..RUNES_ALPHA.len());
            RUNES_ALPHA[idx] as char
        })
        .collect();

    rand_ufrag
}
