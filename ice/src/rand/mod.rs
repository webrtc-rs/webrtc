#[cfg(test)]
mod rand_test;

use rand::{thread_rng, Rng};

const RUNES_ALPHA: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
const RUNES_CANDIDATE_ID_FOUNDATION: &[u8] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789/+";

const LEN_UFRAG: usize = 16;
const LEN_PWD: usize = 32;

// TODO: cryptographically strong random source
pub fn generate_crypto_random_string(n: usize, runes: &[u8]) -> String {
    let mut rng = thread_rng();

    let rand_string: String = (0..n)
        .map(|_| {
            let idx = rng.gen_range(0..runes.len());
            runes[idx] as char
        })
        .collect();

    rand_string
}

/// <https://tools.ietf.org/html/rfc5245#section-15.1>
/// candidate-id = "candidate" ":" foundation
/// foundation   = 1*32ice-char
/// ice-char     = ALPHA / DIGIT / "+" / "/"
pub fn generate_cand_id() -> String {
    format!(
        "candidate:{}",
        generate_crypto_random_string(32, RUNES_CANDIDATE_ID_FOUNDATION)
    )
}

/// Generates ICE pwd.
/// This internally uses `generate_crypto_random_string`.
pub fn generate_pwd() -> String {
    generate_crypto_random_string(LEN_PWD, RUNES_ALPHA)
}

/// ICE user fragment.
/// This internally uses `generate_crypto_random_string`.
pub fn generate_ufrag() -> String {
    generate_crypto_random_string(LEN_UFRAG, RUNES_ALPHA)
}
