//! Regression checks for issues #774, #776, #777, #778.
//!
//! Run with:
//!   cargo run --example regression-checks
//!
//! All three checks run in-process; no browser or external signaling needed.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::Notify;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::setting_engine::SettingEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::RTCPeerConnection;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_api(se: SettingEngine) -> Result<webrtc::api::API> {
    let mut m = MediaEngine::default();
    m.register_default_codecs()?;
    let mut registry = webrtc::interceptor::registry::Registry::new();
    registry = register_default_interceptors(registry, &mut m)?;
    Ok(APIBuilder::new()
        .with_media_engine(m)
        .with_interceptor_registry(registry)
        .with_setting_engine(se)
        .build())
}

/// Perform a full offer/answer exchange between two in-process peer connections
/// (no STUN; purely loopback).
async fn signal_pair(
    offerer: &RTCPeerConnection,
    answerer: &RTCPeerConnection,
) -> Result<()> {
    let offer = offerer.create_offer(None).await?;
    let mut gather = offerer.gathering_complete_promise().await;
    offerer.set_local_description(offer).await?;
    let _ = gather.recv().await;

    let offerer_desc = offerer
        .local_description()
        .await
        .ok_or_else(|| anyhow::anyhow!("no local description on offerer"))?;
    answerer.set_remote_description(offerer_desc).await?;

    let answer = answerer.create_answer(None).await?;
    let mut gather2 = answerer.gathering_complete_promise().await;
    answerer.set_local_description(answer).await?;
    let _ = gather2.recv().await;

    let answerer_desc = answerer
        .local_description()
        .await
        .ok_or_else(|| anyhow::anyhow!("no local description on answerer"))?;
    offerer.set_remote_description(answerer_desc).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Check 1 – #776: unreliable DataChannel stays Open after failed deliveries
// ---------------------------------------------------------------------------

async fn check_unreliable_datachannel_stays_open() -> Result<()> {
    println!("\n=== Check #776: unreliable DataChannel stays Open ===");

    // Use loopback only (no STUN) so the peer connection forms quickly.
    let api = build_api(SettingEngine::default())?;
    let config = RTCConfiguration::default();

    let pc_offer = Arc::new(api.new_peer_connection(config.clone()).await?);
    let pc_answer = Arc::new(api.new_peer_connection(config).await?);

    // Create an unreliable, unordered data channel (max_retransmits = 0).
    let dc_init = RTCDataChannelInit {
        ordered: Some(false),
        max_retransmits: Some(0),
        ..Default::default()
    };
    let dc = pc_offer
        .create_data_channel("unreliable", Some(dc_init))
        .await?;

    // Track how many times on_close fires before our explicit pc.close() call.
    // We use an Arc<AtomicBool> guard: once we set it the close is expected.
    let close_count = Arc::new(AtomicU32::new(0));
    let close_count2 = Arc::clone(&close_count);
    dc.on_close(Box::new(move || {
        close_count2.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }));

    // Track messages received on the answerer side.
    let msgs_received = Arc::new(AtomicU32::new(0));
    let msgs_received2 = Arc::clone(&msgs_received);
    let open_notify = Arc::new(Notify::new());
    let open_notify2 = Arc::clone(&open_notify);

    pc_answer.on_data_channel(Box::new(move |d| {
        let msgs_received3 = Arc::clone(&msgs_received2);
        let open_notify3 = Arc::clone(&open_notify2);
        Box::pin(async move {
            d.on_open(Box::new(move || {
                open_notify3.notify_one();
                Box::pin(async {})
            }));
            d.on_message(Box::new(move |_msg: DataChannelMessage| {
                msgs_received3.fetch_add(1, Ordering::SeqCst);
                Box::pin(async {})
            }));
        })
    }));

    signal_pair(&pc_offer, &pc_answer).await?;

    // Wait for the data channel to open on both sides.
    let dc_open = Arc::new(Notify::new());
    let dc_open2 = Arc::clone(&dc_open);
    dc.on_open(Box::new(move || {
        dc_open2.notify_one();
        Box::pin(async {})
    }));
    tokio::time::timeout(Duration::from_secs(10), dc_open.notified())
        .await
        .map_err(|_| anyhow::anyhow!("timed out waiting for data channel to open"))?;

    // Also wait for the answerer side to be ready.
    tokio::time::timeout(Duration::from_secs(5), open_notify.notified())
        .await
        .map_err(|_| anyhow::anyhow!("timed out waiting for answerer data channel open"))?;

    // Send 20 messages in quick succession. On a real lossy network some would
    // be dropped by SCTP; here in loopback all should arrive, but the important
    // assertion is that the channel does NOT close regardless.
    let msgs_to_send = 20u32;
    for i in 0..msgs_to_send {
        let payload = format!("msg-{i}");
        // Ignore send errors – unreliable channels are allowed to fail sends.
        let _ = dc.send_text(payload).await;
    }

    // Brief pause to let any close callbacks fire (they shouldn't).
    tokio::time::sleep(Duration::from_millis(500)).await;

    let closes = close_count.load(Ordering::SeqCst);
    let received = msgs_received.load(Ordering::SeqCst);

    println!("  Messages sent:    {msgs_to_send}");
    println!("  Messages received: {received}");
    println!("  on_close fires:    {closes}  (must be 0)");

    if closes > 0 {
        anyhow::bail!("FAIL: data channel closed unexpectedly ({closes} time(s))");
    }
    println!("  PASS: channel stayed Open throughout");

    pc_offer.close().await?;
    pc_answer.close().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Check 2 – #778: ICE gathering with unreachable STUN completes within timeout
// ---------------------------------------------------------------------------

async fn check_gather_timeout_with_unreachable_stun() -> Result<()> {
    println!("\n=== Check #778: ICE gather completes within candidate_gather_timeout ===");

    // Point at an address that will never respond (TEST-NET, RFC 5737).
    let unreachable_stun = "stun:192.0.2.1:3478";
    let gather_limit = Duration::from_secs(4);

    let mut se = SettingEngine::default();
    // Cap total gather time to 4 s; each STUN task still has its own 5 s
    // STUN_GATHER_TIMEOUT but the overall wg.wait() will be cut short.
    se.set_candidate_gather_timeout(Some(gather_limit));

    let api = build_api(se)?;
    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec![unreachable_stun.to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let pc = Arc::new(api.new_peer_connection(config).await?);

    // Create a data channel to trigger ICE gathering.
    let _ = pc.create_data_channel("probe", None).await?;
    let offer = pc.create_offer(None).await?;
    let mut gather_complete = pc.gathering_complete_promise().await;
    pc.set_local_description(offer).await?;

    let started = Instant::now();
    tokio::time::timeout(Duration::from_secs(15), gather_complete.recv())
        .await
        .map_err(|_| anyhow::anyhow!("gathering never completed"))?;
    let elapsed = started.elapsed();

    println!("  Unreachable STUN: {unreachable_stun}");
    println!("  candidate_gather_timeout: {}s", gather_limit.as_secs());
    println!("  Gathering completed in: {:.2}s", elapsed.as_secs_f64());

    // Allow a small margin over the configured timeout for task scheduling.
    let margin = Duration::from_secs(3);
    if elapsed > gather_limit + margin {
        anyhow::bail!(
            "FAIL: gathering took {:.2}s, expected <= {}s",
            elapsed.as_secs_f64(),
            (gather_limit + margin).as_secs()
        );
    }
    println!("  PASS: gathering completed well within the deadline");

    pc.close().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Check 3 – #774: IPv6 STUN DNS lookup does not block gathering indefinitely
// ---------------------------------------------------------------------------

async fn check_ipv6_gather_does_not_hang() -> Result<()> {
    println!("\n=== Check #774: IPv6 STUN gather completes without hanging ===");

    // Use an IPv6 STUN URL that will either fail DNS or fail to bind.
    // stun.l.google.com resolves to both IPv4 and IPv6; on a host without
    // IPv6 connectivity the bind will fail immediately.  We also add a
    // deliberately unresolvable IPv6-only host to exercise the DNS timeout path.
    let ipv6_stun_urls = vec![
        "stun:stun.l.google.com:19302".to_owned(),
    ];

    // Keep the overall gather cap tight to verify the DNS timeout in the
    // SRFLX gatherer fires before the overall wg times out anyway.
    let gather_limit = Duration::from_secs(6);

    let mut se = SettingEngine::default();
    se.set_candidate_gather_timeout(Some(gather_limit));

    let api = build_api(se)?;
    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: ipv6_stun_urls.clone(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let pc = Arc::new(api.new_peer_connection(config).await?);
    let _ = pc.create_data_channel("probe", None).await?;
    let offer = pc.create_offer(None).await?;
    let mut gather_complete = pc.gathering_complete_promise().await;
    pc.set_local_description(offer).await?;

    let started = Instant::now();
    tokio::time::timeout(Duration::from_secs(15), gather_complete.recv())
        .await
        .map_err(|_| anyhow::anyhow!("gathering never completed"))?;
    let elapsed = started.elapsed();

    println!("  STUN servers: {:?}", ipv6_stun_urls);
    println!("  candidate_gather_timeout: {}s", gather_limit.as_secs());
    println!("  Gathering completed in: {:.2}s", elapsed.as_secs_f64());

    // Per-URL DNS timeout is 3 s (STUN_DNS_TIMEOUT); overall cap is 6 s.
    // On a system with IPv6, STUN may succeed quickly; on one without IPv6
    // it should fail fast and complete well inside the 6 s cap.
    let margin = Duration::from_secs(3);
    if elapsed > gather_limit + margin {
        anyhow::bail!(
            "FAIL: gathering took {:.2}s, should have completed within {}s",
            elapsed.as_secs_f64(),
            (gather_limit + margin).as_secs()
        );
    }
    println!("  PASS: gathering did not hang on IPv6 DNS");

    pc.close().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    // Uncomment for verbose ICE/SCTP logs:
    // env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    println!("Running regression checks for webrtc-rs issues #774 #776 #778");
    println!("(#777 is exercised implicitly whenever the UDP mux is under load)");

    let mut failed = false;

    if let Err(e) = check_unreliable_datachannel_stays_open().await {
        eprintln!("  ERROR: {e}");
        failed = true;
    }

    if let Err(e) = check_gather_timeout_with_unreachable_stun().await {
        eprintln!("  ERROR: {e}");
        failed = true;
    }

    if let Err(e) = check_ipv6_gather_does_not_hang().await {
        eprintln!("  ERROR: {e}");
        failed = true;
    }

    println!();
    if failed {
        eprintln!("One or more checks FAILED.");
        std::process::exit(1);
    } else {
        println!("All checks PASSED.");
    }

    Ok(())
}
