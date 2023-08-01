#[cfg(test)]
mod handshake_cache_test;

use std::collections::HashMap;
use std::io::BufReader;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::cipher_suite::*;
use crate::handshake::*;

#[derive(Clone, Debug)]
pub(crate) struct HandshakeCacheItem {
    typ: HandshakeType,
    is_client: bool,
    epoch: u16,
    message_sequence: u16,
    data: Vec<u8>,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct HandshakeCachePullRule {
    pub(crate) typ: HandshakeType,
    pub(crate) epoch: u16,
    pub(crate) is_client: bool,
    pub(crate) optional: bool,
}

#[derive(Clone)]
pub(crate) struct HandshakeCache {
    cache: Arc<Mutex<Vec<HandshakeCacheItem>>>,
}

impl HandshakeCache {
    pub(crate) fn new() -> Self {
        HandshakeCache {
            cache: Arc::new(Mutex::new(vec![])),
        }
    }

    pub(crate) async fn push(
        &mut self,
        data: Vec<u8>,
        epoch: u16,
        message_sequence: u16,
        typ: HandshakeType,
        is_client: bool,
    ) -> bool {
        let mut cache = self.cache.lock().await;

        for i in &*cache {
            if i.message_sequence == message_sequence && i.is_client == is_client {
                return false;
            }
        }

        cache.push(HandshakeCacheItem {
            typ,
            is_client,
            epoch,
            message_sequence,
            data,
        });

        true
    }

    // returns a list handshakes that match the requested rules
    // the list will contain null entries for rules that can't be satisfied
    // multiple entries may match a rule, but only the last match is returned (ie ClientHello with cookies)
    pub(crate) async fn pull(&self, rules: &[HandshakeCachePullRule]) -> Vec<HandshakeCacheItem> {
        let cache = self.cache.lock().await;

        let mut out = vec![];
        for r in rules {
            let mut item: Option<HandshakeCacheItem> = None;
            for c in &*cache {
                if c.typ == r.typ && c.is_client == r.is_client && c.epoch == r.epoch {
                    if let Some(x) = &item {
                        if x.message_sequence < c.message_sequence {
                            item = Some(c.clone());
                        }
                    } else {
                        item = Some(c.clone());
                    }
                }
            }

            if let Some(c) = item {
                out.push(c);
            }
        }

        out
    }

    // full_pull_map pulls all handshakes between rules[0] to rules[len(rules)-1] as map.
    pub(crate) async fn full_pull_map(
        &self,
        start_seq: isize,
        rules: &[HandshakeCachePullRule],
    ) -> Result<(isize, HashMap<HandshakeType, HandshakeMessage>)> {
        let cache = self.cache.lock().await;

        let mut ci = HashMap::new();
        for r in rules {
            let mut item: Option<HandshakeCacheItem> = None;
            for c in &*cache {
                if c.typ == r.typ && c.is_client == r.is_client && c.epoch == r.epoch {
                    if let Some(x) = &item {
                        if x.message_sequence < c.message_sequence {
                            item = Some(c.clone());
                        }
                    } else {
                        item = Some(c.clone());
                    }
                }
            }
            if !r.optional && item.is_none() {
                // Missing mandatory message.
                return Err(Error::Other("Missing mandatory message".to_owned()));
            }

            if let Some(c) = item {
                ci.insert(r.typ, c);
            }
        }

        let mut out = HashMap::new();
        let mut seq = start_seq;
        for r in rules {
            let t = r.typ;
            if let Some(i) = ci.get(&t) {
                let mut reader = BufReader::new(i.data.as_slice());
                let raw_handshake = Handshake::unmarshal(&mut reader)?;
                if seq as u16 != raw_handshake.handshake_header.message_sequence {
                    // There is a gap. Some messages are not arrived.
                    return Err(Error::Other(
                        "There is a gap. Some messages are not arrived.".to_owned(),
                    ));
                }
                seq += 1;
                out.insert(t, raw_handshake.handshake_message);
            }
        }

        Ok((seq, out))
    }

    // pull_and_merge calls pull and then merges the results, ignoring any null entries
    pub(crate) async fn pull_and_merge(&self, rules: &[HandshakeCachePullRule]) -> Vec<u8> {
        let mut merged = vec![];

        for p in &self.pull(rules).await {
            merged.extend_from_slice(&p.data);
        }

        merged
    }

    // session_hash returns the session hash for Extended Master Secret support
    // https://tools.ietf.org/html/draft-ietf-tls-session-hash-06#section-4
    pub(crate) async fn session_hash(
        &self,
        hf: CipherSuiteHash,
        epoch: u16,
        additional: &[u8],
    ) -> Result<Vec<u8>> {
        let mut merged = vec![];

        // Order defined by https://tools.ietf.org/html/rfc5246#section-7.3
        let handshake_buffer = self
            .pull(&[
                HandshakeCachePullRule {
                    typ: HandshakeType::ClientHello,
                    epoch,
                    is_client: true,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: HandshakeType::ServerHello,
                    epoch,
                    is_client: false,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: HandshakeType::Certificate,
                    epoch,
                    is_client: false,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: HandshakeType::ServerKeyExchange,
                    epoch,
                    is_client: false,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: HandshakeType::CertificateRequest,
                    epoch,
                    is_client: false,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: HandshakeType::ServerHelloDone,
                    epoch,
                    is_client: false,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: HandshakeType::Certificate,
                    epoch,
                    is_client: true,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: HandshakeType::ClientKeyExchange,
                    epoch,
                    is_client: true,
                    optional: false,
                },
            ])
            .await;

        for p in &handshake_buffer {
            merged.extend_from_slice(&p.data);
        }

        merged.extend_from_slice(additional);

        let mut hasher = match hf {
            CipherSuiteHash::Sha256 => Sha256::new(),
        };
        hasher.update(&merged);
        let result = hasher.finalize();

        Ok(result.as_slice().to_vec())
    }
}
