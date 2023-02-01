use super::*;
use std::sync::atomic::AtomicU16;

#[tokio::test]
async fn test_generate_data_channel_id() -> Result<()> {
    let sctp_transport_with_channels = |ids: &[u16]| -> RTCSctpTransport {
        let mut data_channels = vec![];
        for id in ids {
            data_channels.push(Arc::new(RTCDataChannel {
                id: AtomicU16::new(*id),
                ..Default::default()
            }));
        }

        RTCSctpTransport {
            data_channels: Arc::new(Mutex::new(data_channels)),
            ..Default::default()
        }
    };

    let tests = vec![
        (DTLSRole::Client, sctp_transport_with_channels(&[]), 0),
        (DTLSRole::Client, sctp_transport_with_channels(&[1]), 0),
        (DTLSRole::Client, sctp_transport_with_channels(&[0]), 2),
        (DTLSRole::Client, sctp_transport_with_channels(&[0, 2]), 4),
        (DTLSRole::Client, sctp_transport_with_channels(&[0, 4]), 2),
        (DTLSRole::Server, sctp_transport_with_channels(&[]), 1),
        (DTLSRole::Server, sctp_transport_with_channels(&[0]), 1),
        (DTLSRole::Server, sctp_transport_with_channels(&[1]), 3),
        (DTLSRole::Server, sctp_transport_with_channels(&[1, 3]), 5),
        (DTLSRole::Server, sctp_transport_with_channels(&[1, 5]), 3),
    ];

    for (role, s, expected) in tests {
        match s.generate_and_set_data_channel_id(role).await {
            Ok(actual) => assert_eq!(actual, expected),
            Err(err) => panic!("failed to generate id: {err}"),
        };
    }

    Ok(())
}
