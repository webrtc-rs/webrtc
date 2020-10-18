use rand::Rng;

use std::time::{Duration, SystemTime};

const RANDOM_BYTES_LENGTH: usize = 28;
const HANDSHAKE_RANDOM_LENGTH: usize = RANDOM_BYTES_LENGTH + 4;

// https://tools.ietf.org/html/rfc4346#section-7.4.1.2
#[derive(Clone, Debug, PartialEq)]
pub struct HandshakeRandom {
    gmt_unix_time: SystemTime,
    random_bytes: [u8; RANDOM_BYTES_LENGTH],
}

impl HandshakeRandom {
    pub fn marshal_fixed(&self) -> [u8; HANDSHAKE_RANDOM_LENGTH] {
        let mut out = [0u8; HANDSHAKE_RANDOM_LENGTH];

        let secs = match self.gmt_unix_time.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(d) => d.as_secs() as u32,
            Err(_) => 0,
        };
        out[0..4].copy_from_slice(&secs.to_be_bytes());
        out[4..].copy_from_slice(&self.random_bytes);

        out
    }

    pub fn unmarshal_fixed(data: &[u8; HANDSHAKE_RANDOM_LENGTH]) -> Self {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&data[0..4]);

        let secs = u32::from_be_bytes(bytes);
        let gmt_unix_time = if let Some(unix_time) =
            SystemTime::UNIX_EPOCH.checked_add(Duration::new(secs as u64, 0))
        {
            unix_time
        } else {
            SystemTime::UNIX_EPOCH
        };

        let mut handshake_random = HandshakeRandom {
            gmt_unix_time,
            random_bytes: [0u8; RANDOM_BYTES_LENGTH],
        };

        handshake_random.random_bytes.copy_from_slice(&data[4..]);

        handshake_random
    }

    // populate fills the HandshakeRandom with random values
    // may be called multiple times
    pub fn populate(&mut self) {
        self.gmt_unix_time = SystemTime::now();
        rand::thread_rng().fill(&mut self.random_bytes);
    }
}
