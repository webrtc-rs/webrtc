use super::*;
use crate::api::APIBuilder;
use crate::peer::configuration::Configuration;
use crate::peer::peer_connection::peer_connection_test::{
    close_pair_now, signal_pair, until_connection_state,
};
use crate::peer::peer_connection_state::PeerConnectionState;
use ice::mdns::MulticastDnsMode;
use ice::network_type::NetworkType;
use waitgroup::WaitGroup;

/*TODO: TestInvalidFingerprintCausesFailed
// An invalid fingerprint MUST cause PeerConnectionState to go to PeerConnectionStateFailed
func TestInvalidFingerprintCausesFailed(t *testing.T) {
    lim := test.TimeOut(time.Second * 40)
    defer lim.Stop()

    report := test.CheckRoutines(t)
    defer report()

    pcOffer, err := NewPeerConnection(Configuration{})
    if err != nil {
        t.Fatal(err)
    }

    pcAnswer, err := NewPeerConnection(Configuration{})
    if err != nil {
        t.Fatal(err)
    }

    pcAnswer.OnDataChannel(func(_ *DataChannel) {
        t.Fatal("A DataChannel must not be created when Fingerprint verification fails")
    })

    defer closePairNow(t, pcOffer, pcAnswer)

    offerChan := make(chan SessionDescription)
    pcOffer.OnICECandidate(func(candidate *ICECandidate) {
        if candidate == nil {
            offerChan <- *pcOffer.PendingLocalDescription()
        }
    })

    offerConnectionHasFailed := untilConnectionState(PeerConnectionStateFailed, pcOffer)
    answerConnectionHasFailed := untilConnectionState(PeerConnectionStateFailed, pcAnswer)

    if _, err = pcOffer.CreateDataChannel("unusedDataChannel", nil); err != nil {
        t.Fatal(err)
    }

    offer, err := pcOffer.CreateOffer(nil)
    if err != nil {
        t.Fatal(err)
    } else if err := pcOffer.SetLocalDescription(offer); err != nil {
        t.Fatal(err)
    }

    select {
    case offer := <-offerChan:
        // Replace with invalid fingerprint
        re := regexp.MustCompile(`sha-256 (.*?)\r`)
        offer.SDP = re.ReplaceAllString(offer.SDP, "sha-256 AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA\r")

        if err := pcAnswer.SetRemoteDescription(offer); err != nil {
            t.Fatal(err)
        }

        answer, err := pcAnswer.CreateAnswer(nil)
        if err != nil {
            t.Fatal(err)
        }

        if err = pcAnswer.SetLocalDescription(answer); err != nil {
            t.Fatal(err)
        }

        answer.SDP = re.ReplaceAllString(answer.SDP, "sha-256 AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA:AA\r")

        err = pcOffer.SetRemoteDescription(answer)
        if err != nil {
            t.Fatal(err)
        }
    case <-time.After(5 * time.Second):
        t.Fatal("timed out waiting to receive offer")
    }

    offerConnectionHasFailed.Wait()
    answerConnectionHasFailed.Wait()

    assert.Equal(t, pcOffer.SCTP().Transport().State(), DTLSTransportStateFailed)
    assert.Nil(t, pcOffer.SCTP().Transport().conn)

    assert.Equal(t, pcAnswer.SCTP().Transport().State(), DTLSTransportStateFailed)
    assert.Nil(t, pcAnswer.SCTP().Transport().conn)
}
*/

async fn run_test(r: DTLSRole) -> Result<()> {
    let mut offer_s = SettingEngine::default();
    offer_s.set_answering_dtls_role(r)?;
    offer_s.set_ice_multicast_dns_mode(MulticastDnsMode::Disabled);
    offer_s.set_network_types(vec![NetworkType::Udp4]);
    let mut offer_pc = APIBuilder::new()
        .with_setting_engine(offer_s)
        .build()
        .new_peer_connection(Configuration::default())
        .await?;

    let mut answer_s = SettingEngine::default();
    answer_s.set_answering_dtls_role(r)?;
    answer_s.set_ice_multicast_dns_mode(MulticastDnsMode::Disabled);
    answer_s.set_network_types(vec![NetworkType::Udp4]);
    let mut answer_pc = APIBuilder::new()
        .with_setting_engine(answer_s)
        .build()
        .new_peer_connection(Configuration::default())
        .await?;

    signal_pair(&mut offer_pc, &mut answer_pc).await?;

    let wg = WaitGroup::new();
    until_connection_state(&mut answer_pc, &wg, PeerConnectionState::Connected).await;
    wg.wait().await;

    close_pair_now(&offer_pc, &answer_pc).await;

    Ok(())
}

/*TODO: test_peer_connection_dtls_role_setting_engine_server/client
use log::LevelFilter;
use std::io::Write;

#[tokio::test]
async fn test_peer_connection_dtls_role_setting_engine_server() -> Result<()> {
    env_logger::Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{}:{} [{}] {} - {}",
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.level(),
                chrono::Local::now().format("%H:%M:%S.%6f"),
                record.args()
            )
        })
        .filter(None, LevelFilter::Trace)
        .init();

    run_test(DTLSRole::Server).await
}

#[tokio::test]
async fn test_peer_connection_dtls_role_setting_engine_client() -> Result<()> {
    env_logger::Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{}:{} [{}] {} - {}",
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.level(),
                chrono::Local::now().format("%H:%M:%S.%6f"),
                record.args()
            )
        })
        .filter(None, LevelFilter::Trace)
        .init();

    run_test(DTLSRole::Client).await
}
*/
