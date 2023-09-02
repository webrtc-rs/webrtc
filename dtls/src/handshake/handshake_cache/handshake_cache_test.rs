use super::*;

#[tokio::test]
async fn test_handshake_cache_single_push() -> Result<()> {
    let tests = vec![
        (
            "Single Push",
            vec![HandshakeCacheItem {
                typ: 0.into(),
                is_client: true,
                epoch: 0,
                message_sequence: 0,
                data: vec![0x00],
            }],
            vec![HandshakeCachePullRule {
                typ: 0.into(),
                epoch: 0,
                is_client: true,
                optional: false,
            }],
            vec![0x00],
        ),
        (
            "Multi Push",
            vec![
                HandshakeCacheItem {
                    typ: 0.into(),
                    is_client: true,
                    epoch: 0,
                    message_sequence: 0,
                    data: vec![0x00],
                },
                HandshakeCacheItem {
                    typ: 1.into(),
                    is_client: true,
                    epoch: 0,
                    message_sequence: 1,
                    data: vec![0x01],
                },
                HandshakeCacheItem {
                    typ: 2.into(),
                    is_client: true,
                    epoch: 0,
                    message_sequence: 2,
                    data: vec![0x02],
                },
            ],
            vec![
                HandshakeCachePullRule {
                    typ: 0.into(),
                    epoch: 0,
                    is_client: true,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: 1.into(),
                    epoch: 0,
                    is_client: true,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: 2.into(),
                    epoch: 0,
                    is_client: true,
                    optional: false,
                },
            ],
            vec![0x00, 0x01, 0x02],
        ),
        (
            "Multi Push, Rules set order",
            vec![
                HandshakeCacheItem {
                    typ: 2.into(),
                    is_client: true,
                    epoch: 0,
                    message_sequence: 2,
                    data: vec![0x02],
                },
                HandshakeCacheItem {
                    typ: 0.into(),
                    is_client: true,
                    epoch: 0,
                    message_sequence: 0,
                    data: vec![0x00],
                },
                HandshakeCacheItem {
                    typ: 1.into(),
                    is_client: true,
                    epoch: 0,
                    message_sequence: 1,
                    data: vec![0x01],
                },
            ],
            vec![
                HandshakeCachePullRule {
                    typ: 0.into(),
                    epoch: 0,
                    is_client: true,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: 1.into(),
                    epoch: 0,
                    is_client: true,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: 2.into(),
                    epoch: 0,
                    is_client: true,
                    optional: false,
                },
            ],
            vec![0x00, 0x01, 0x02],
        ),
        (
            "Multi Push, Dupe Seqnum",
            vec![
                HandshakeCacheItem {
                    typ: 0.into(),
                    is_client: true,
                    epoch: 0,
                    message_sequence: 0,
                    data: vec![0x00],
                },
                HandshakeCacheItem {
                    typ: 1.into(),
                    is_client: true,
                    epoch: 0,
                    message_sequence: 1,
                    data: vec![0x01],
                },
                HandshakeCacheItem {
                    typ: 1.into(),
                    is_client: true,
                    epoch: 0,
                    message_sequence: 1,
                    data: vec![0x01],
                },
            ],
            vec![
                HandshakeCachePullRule {
                    typ: 0.into(),
                    epoch: 0,
                    is_client: true,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: 1.into(),
                    epoch: 0,
                    is_client: true,
                    optional: false,
                },
            ],
            vec![0x00, 0x01],
        ),
        (
            "Multi Push, Dupe Seqnum Client/Server",
            vec![
                HandshakeCacheItem {
                    typ: 0.into(),
                    is_client: true,
                    epoch: 0,
                    message_sequence: 0,
                    data: vec![0x00],
                },
                HandshakeCacheItem {
                    typ: 1.into(),
                    is_client: true,
                    epoch: 0,
                    message_sequence: 1,
                    data: vec![0x01],
                },
                HandshakeCacheItem {
                    typ: 1.into(),
                    is_client: false,
                    epoch: 0,
                    message_sequence: 1,
                    data: vec![0x02],
                },
            ],
            vec![
                HandshakeCachePullRule {
                    typ: 0.into(),
                    epoch: 0,
                    is_client: true,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: 1.into(),
                    epoch: 0,
                    is_client: true,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: 1.into(),
                    epoch: 0,
                    is_client: false,
                    optional: false,
                },
            ],
            vec![0x00, 0x01, 0x02],
        ),
        (
            "Multi Push, Dupe Seqnum with Unique HandshakeType",
            vec![
                HandshakeCacheItem {
                    typ: 1.into(),
                    is_client: true,
                    epoch: 0,
                    message_sequence: 0,
                    data: vec![0x00],
                },
                HandshakeCacheItem {
                    typ: 2.into(),
                    is_client: true,
                    epoch: 0,
                    message_sequence: 1,
                    data: vec![0x01],
                },
                HandshakeCacheItem {
                    typ: 3.into(),
                    is_client: false,
                    epoch: 0,
                    message_sequence: 0,
                    data: vec![0x02],
                },
            ],
            vec![
                HandshakeCachePullRule {
                    typ: 1.into(),
                    epoch: 0,
                    is_client: true,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: 2.into(),
                    epoch: 0,
                    is_client: true,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: 3.into(),
                    epoch: 0,
                    is_client: false,
                    optional: false,
                },
            ],
            vec![0x00, 0x01, 0x02],
        ),
        (
            "Multi Push, Wrong epoch",
            vec![
                HandshakeCacheItem {
                    typ: 1.into(),
                    is_client: true,
                    epoch: 0,
                    message_sequence: 0,
                    data: vec![0x00],
                },
                HandshakeCacheItem {
                    typ: 2.into(),
                    is_client: true,
                    epoch: 1,
                    message_sequence: 1,
                    data: vec![0x01],
                },
                HandshakeCacheItem {
                    typ: 2.into(),
                    is_client: true,
                    epoch: 0,
                    message_sequence: 2,
                    data: vec![0x11],
                },
                HandshakeCacheItem {
                    typ: 3.into(),
                    is_client: false,
                    epoch: 0,
                    message_sequence: 0,
                    data: vec![0x02],
                },
                HandshakeCacheItem {
                    typ: 3.into(),
                    is_client: false,
                    epoch: 1,
                    message_sequence: 0,
                    data: vec![0x12],
                },
                HandshakeCacheItem {
                    typ: 3.into(),
                    is_client: false,
                    epoch: 2,
                    message_sequence: 0,
                    data: vec![0x12],
                },
            ],
            vec![
                HandshakeCachePullRule {
                    typ: 1.into(),
                    epoch: 0,
                    is_client: true,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: 2.into(),
                    epoch: 1,
                    is_client: true,
                    optional: false,
                },
                HandshakeCachePullRule {
                    typ: 3.into(),
                    epoch: 0,
                    is_client: false,
                    optional: false,
                },
            ],
            vec![0x00, 0x01, 0x02],
        ),
    ];

    for (name, inputs, rules, expected) in tests {
        let mut h = HandshakeCache::new();
        for i in inputs {
            h.push(i.data, i.epoch, i.message_sequence, i.typ, i.is_client)
                .await;
        }
        let verify_data = h.pull_and_merge(&rules).await;
        assert_eq!(
            verify_data, expected,
            "handshakeCache '{name}' exp:{expected:?} actual {verify_data:?}",
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_handshake_cache_session_hash() -> Result<()> {
    let tests = vec![
        (
            "Standard Handshake",
            vec![
                HandshakeCacheItem {
                    typ: HandshakeType::ClientHello,
                    is_client: true,
                    epoch: 0,
                    message_sequence: 0,
                    data: vec![0x00],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::ServerHello,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 1,
                    data: vec![0x01],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::Certificate,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 2,
                    data: vec![0x02],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::ServerKeyExchange,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 3,
                    data: vec![0x03],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::ServerHelloDone,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 4,
                    data: vec![0x04],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::ClientKeyExchange,
                    is_client: true,
                    epoch: 0,
                    message_sequence: 5,
                    data: vec![0x05],
                },
            ],
            vec![
                0x17, 0xe8, 0x8d, 0xb1, 0x87, 0xaf, 0xd6, 0x2c, 0x16, 0xe5, 0xde, 0xbf, 0x3e, 0x65,
                0x27, 0xcd, 0x00, 0x6b, 0xc0, 0x12, 0xbc, 0x90, 0xb5, 0x1a, 0x81, 0x0c, 0xd8, 0x0c,
                0x2d, 0x51, 0x1f, 0x43,
            ],
        ),
        (
            "Handshake With Client Cert Request",
            vec![
                HandshakeCacheItem {
                    typ: HandshakeType::ClientHello,
                    is_client: true,
                    epoch: 0,
                    message_sequence: 0,
                    data: vec![0x00],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::ServerHello,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 1,
                    data: vec![0x01],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::Certificate,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 2,
                    data: vec![0x02],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::ServerKeyExchange,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 3,
                    data: vec![0x03],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::CertificateRequest,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 4,
                    data: vec![0x04],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::ServerHelloDone,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 5,
                    data: vec![0x05],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::ClientKeyExchange,
                    is_client: true,
                    epoch: 0,
                    message_sequence: 6,
                    data: vec![0x06],
                },
            ],
            vec![
                0x57, 0x35, 0x5a, 0xc3, 0x30, 0x3c, 0x14, 0x8f, 0x11, 0xae, 0xf7, 0xcb, 0x17, 0x94,
                0x56, 0xb9, 0x23, 0x2c, 0xde, 0x33, 0xa8, 0x18, 0xdf, 0xda, 0x2c, 0x2f, 0xcb, 0x93,
                0x25, 0x74, 0x9a, 0x6b,
            ],
        ),
        (
            "Handshake Ignores after ClientKeyExchange",
            vec![
                HandshakeCacheItem {
                    typ: HandshakeType::ClientHello,
                    is_client: true,
                    epoch: 0,
                    message_sequence: 0,
                    data: vec![0x00],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::ServerHello,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 1,
                    data: vec![0x01],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::Certificate,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 2,
                    data: vec![0x02],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::ServerKeyExchange,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 3,
                    data: vec![0x03],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::CertificateRequest,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 4,
                    data: vec![0x04],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::ServerHelloDone,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 5,
                    data: vec![0x05],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::ClientKeyExchange,
                    is_client: true,
                    epoch: 0,
                    message_sequence: 6,
                    data: vec![0x06],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::CertificateVerify,
                    is_client: true,
                    epoch: 0,
                    message_sequence: 7,
                    data: vec![0x07],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::Finished,
                    is_client: true,
                    epoch: 1,
                    message_sequence: 7,
                    data: vec![0x08],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::Finished,
                    is_client: false,
                    epoch: 1,
                    message_sequence: 7,
                    data: vec![0x09],
                },
            ],
            vec![
                0x57, 0x35, 0x5a, 0xc3, 0x30, 0x3c, 0x14, 0x8f, 0x11, 0xae, 0xf7, 0xcb, 0x17, 0x94,
                0x56, 0xb9, 0x23, 0x2c, 0xde, 0x33, 0xa8, 0x18, 0xdf, 0xda, 0x2c, 0x2f, 0xcb, 0x93,
                0x25, 0x74, 0x9a, 0x6b,
            ],
        ),
        (
            "Handshake Ignores wrong epoch",
            vec![
                HandshakeCacheItem {
                    typ: HandshakeType::ClientHello,
                    is_client: true,
                    epoch: 0,
                    message_sequence: 0,
                    data: vec![0x00],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::ServerHello,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 1,
                    data: vec![0x01],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::Certificate,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 2,
                    data: vec![0x02],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::ServerKeyExchange,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 3,
                    data: vec![0x03],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::CertificateRequest,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 4,
                    data: vec![0x04],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::ServerHelloDone,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 5,
                    data: vec![0x05],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::ClientKeyExchange,
                    is_client: true,
                    epoch: 0,
                    message_sequence: 6,
                    data: vec![0x06],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::CertificateVerify,
                    is_client: true,
                    epoch: 0,
                    message_sequence: 7,
                    data: vec![0x07],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::Finished,
                    is_client: true,
                    epoch: 0,
                    message_sequence: 7,
                    data: vec![0xf0],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::Finished,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 7,
                    data: vec![0xf1],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::Finished,
                    is_client: true,
                    epoch: 1,
                    message_sequence: 7,
                    data: vec![0x08],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::Finished,
                    is_client: false,
                    epoch: 1,
                    message_sequence: 7,
                    data: vec![0x09],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::Finished,
                    is_client: true,
                    epoch: 0,
                    message_sequence: 7,
                    data: vec![0xf0],
                },
                HandshakeCacheItem {
                    typ: HandshakeType::Finished,
                    is_client: false,
                    epoch: 0,
                    message_sequence: 7,
                    data: vec![0xf1],
                },
            ],
            vec![
                0x57, 0x35, 0x5a, 0xc3, 0x30, 0x3c, 0x14, 0x8f, 0x11, 0xae, 0xf7, 0xcb, 0x17, 0x94,
                0x56, 0xb9, 0x23, 0x2c, 0xde, 0x33, 0xa8, 0x18, 0xdf, 0xda, 0x2c, 0x2f, 0xcb, 0x93,
                0x25, 0x74, 0x9a, 0x6b,
            ],
        ),
    ];

    for (name, inputs, expected) in tests {
        let mut h = HandshakeCache::new();
        for i in inputs {
            h.push(i.data, i.epoch, i.message_sequence, i.typ, i.is_client)
                .await;
        }

        let verify_data = h.session_hash(CipherSuiteHash::Sha256, 0, &[]).await?;

        assert_eq!(
            verify_data, expected,
            "handshakeCacheSessionHassh '{name}' exp: {expected:?} actual {verify_data:?}"
        );
    }

    Ok(())
}
