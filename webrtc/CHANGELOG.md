# webrtc-rs changelog

## Unreleased

* Added support for insecure/deprecated signature verification algorithms, opt in via `SettingsEngine::allow_insecure_verification_algorithm` [#342](https://github.com/webrtc-rs/webrtc/pull/342).
* Make RTCRtpCodecCapability::payloader_for_codec public API [#349](https://github.com/webrtc-rs/webrtc/pull/349).
* Fixed a panic in `calculate_rtt_ms` [#350](https://github.com/webrtc-rs/webrtc/pull/350).
* Fixed `TrackRemote` missing at least the first, sometimes more, RTP packet during probing. [#387](https://github.com/webrtc-rs/webrtc/pull/387)

### Breaking changes

* Change `RTCPeerConnection::on_track` callback signature to `|track: Arc<TrackRemote>, receiver: Arc<RTCRtpReceiver>, transceiver: Arc<RTCRtpTransceiver>|` [#355](https://github.com/webrtc-rs/webrtc/pull/355).

* Change `RTCRtpSender::new` signature to `|receive_mtu: usize, track: Option<Arc<dyn TrackLocal + Send + Sync>>, transport: Arc<RTCDtlsTransport>, media_engine: Arc<MediaEngine>, interceptor: Arc<dyn Interceptor + Send + Sync>, start_paused: bool,|` [#377](https://github.com/webrtc-rs/webrtc/pull/377).

* Change `API::new_rtp_sender` signature to `|&self, track: Option<Arc<dyn TrackLocal + Send + Sync>>, transport: Arc<RTCDtlsTransport>, interceptor: Arc<dyn Interceptor + Send + Sync>,|` [#377](https://github.com/webrtc-rs/webrtc/pull/377).

* Change `RTCRtpTransceiver::sender` signature to `|&self| -> Arc<RTCRtpSender>` [#377](https://github.com/webrtc-rs/webrtc/pull/377).

* Change `RTCRtpTransceiver::set_sender_track` signature to `|self: &Arc<Self>, sender: Arc<RTCRtpSender>, track: Option<Arc<dyn TrackLocal + Send + Sync>>,|` [#377](https://github.com/webrtc-rs/webrtc/pull/377).

* Change `RTCRtpTransceiver::set_sender` signature to `|self: &Arc<Self>, s: Arc<RTCRtpSender>|` [#377](https://github.com/webrtc-rs/webrtc/pull/377).

* Change `RTCRtpTransceiver::receiver` signature to `|&self| -> Arc<RTCRtpReceiver>` [#377](https://github.com/webrtc-rs/webrtc/pull/377).

* Change `RTCRtpTransceiver::set_receiver` signature to `|&self, r: Arc<RTCRtpReceiver>|` [#377](https://github.com/webrtc-rs/webrtc/pull/377).

* Change `RTCPeerConnection::add_transceiver_from_kind` signature to `|&self, kind: RTPCodecType, init: Option<RTCRtpTransceiverInit>,|`, `RTCRtpTransceiver::RTCRtpSender` сreated without a track [#377](https://github.com/webrtc-rs/webrtc/pull/377).

* Change `RTCPeerConnection::add_transceiver_from_track` signature to `|&self, track: Arc<dyn TrackLocal + Send + Sync>, init: Option<RTCRtpTransceiverInit>,|` [#377](https://github.com/webrtc-rs/webrtc/pull/377).

* Change `RTCPeerConnection::mid` return signature to `Option<String>` [#375](https://github.com/webrtc-rs/webrtc/pull/375).

* Make functions non-async [#402](https://github.com/webrtc-rs/webrtc/pull/402):
    - `MediaEngine`:
        - `get_codecs_by_kind`;
        - `get_rtp_parameters_by_kind`.
    - `RTCRtpTransceiver`:
        - `sender`;
        - `set_sender`;
        - `receiver`.
    - `RTPReceiverInternal`:
        - `set_transceiver_codecs`;
        - `get_codecs`.
    - `RTCRtpSender`:
        - `set_rtp_transceiver`;
        - `has_sent`.
    - `TrackRemote`:
        - `id`;
        - `set_id`;
        - `stream_id`;
        - `set_stream_id`;
        - `msid`;
        - `codec`;
        - `set_codec`;
        - `params`;
        - `set_params`;
        - `onmute`;
        - `onunmute`.

## v0.6.0

* Added more stats to `RemoteInboundRTPStats` and `RemoteOutboundRTPStats` [#282](https://github.com/webrtc-rs/webrtc/pull/282) by [@k0nserv](https://github.com/k0nserv).
* Don't register `video/rtx` codecs in `MediaEngine::register_default_codecs`. These weren't actually support and prevented RTX in the existing RTP stream from being used. Long term we should support RTX via this method, this is tracked in [#295](https://github.com/webrtc-rs/webrtc/issues/295). [#294 Remove video/rtx codecs](https://github.com/webrtc-rs/webrtc/pull/294) contributed by [k0nserv](https://github.com/k0nserv)
* Add IP filter to WebRTC `SettingEngine` [#306](https://github.com/webrtc-rs/webrtc/pull/306)
* Stop sequence numbers from increasing in `TrackLocalStaticSample` while the bound `RTCRtpSender` have
directions that should not send. [#316](https://github.com/webrtc-rs/webrtc/pull/316)
* Add support for a mime type "audio/telephone-event" (rfc4733) [#322](https://github.com/webrtc-rs/webrtc/pull/322)
* Fixed a panic that would sometimes happen when collecting stats. [#327](https://github.com/webrtc-rs/webrtc/pull/327) by [@k0nserv](https://github.com/k0nserv).
* Added new extension marshaller/unmarshaller for VideoOrientation, and made marshallers serializable via serde [#331](https://github.com/webrtc-rs/webrtc/pull/331) [#332](https://github.com/webrtc-rs/webrtc/pull/332)
* Updated minimum rust version to `1.60.0`
* Added a new `write_rtp_with_extensions` method to `TrackLocalStaticSample` and `TrackLocalStaticRTP`. [#336](https://github.com/webrtc-rs/webrtc/pull/336) by [@k0nserv](https://github.com/k0nserv).
* Added a new `sample_writer` helper to `TrackLocalStaticSample`. [#336](https://github.com/webrtc-rs/webrtc/pull/336) by [@k0nserv](https://github.com/k0nserv).
* Increased minimum versions for sub-dependencies:
  * `webrtc-data` version to `0.6.0`.
  * `webrtc-ice` version to `0.9.0`.
  * `webrtc-media` version to `0.5.0`.
  * `webrtc-sctp` version to `0.7.0`.
  * `webrtc-util` version to `0.7.0`.

### Breaking changes

* Allowed one single direction for extmap matching. [#321](https://github.com/webrtc-rs/webrtc/pull/321). API change for `MediaEngine::register_header_extension`.
* Removed support for Plan-B. All major implementations of WebRTC now support unified and continuing support for plan-b is an undue maintenance burden when unified can be used. See [“Unified Plan” Transition Guide (JavaScript)](https://docs.google.com/document/d/1-ZfikoUtoJa9k-GZG1daN0BU3IjIanQ_JSscHxQesvU/) for an overview of the changes required to migrate. [#320](https://github.com/webrtc-rs/webrtc/pull/320) by [@algesten](https://github.com/algesten).
* Removed 2nd argument from `RTCCertificate::from_pem` and guard it with `pem` feature [#333]
* Renamed `RTCCertificate::pem` to `serialize_pem`  and guard it with `pem` feature [#333]
* Removed `RTCCertificate::expires` [#333]
* `RTCCertificate::get_fingerprints` no longer returns `Result` [#333]
* Make functions non-async [#338](https://github.com/webrtc-rs/webrtc/pull/338):
    - `RTCDataChannel`:
        - `on_open`;
        - `on_close`;
        - `on_message`;
        - `on_error`.
    - `RTCDtlsTransport::on_state_change`;
    - `RTCIceCandidate::to_json`;
    - `RTCIceGatherer`:
        - `on_local_candidate`;
        - `on_state_change`;
        - `on_gathering_complete`.
    - `RTCIceTransport`:
        - `get_selected_candidate_pair`;
        - `on_selected_candidate_pair_change`;
        - `on_connection_state_change`.
    - `RTCPeerConnection`:
        - `on_signaling_state_change`;
        - `on_data_channel`;
        - `on_negotiation_needed`;
        - `on_ice_candidate`;
        - `on_ice_gathering_state_change`;
        - `on_track`;
        - `on_ice_connection_state_change`;
        - `on_peer_connection_state_change`.
    - `RTCSctpTransport`:
        - `on_error`;
        - `on_data_channel`;
        - `on_data_channel_opened`.

[#333]: https://github.com/webrtc-rs/webrtc/pull/333

## v0.5.1

* Promote agent lock in ice_gather.rs create_agent() to top level of the function to avoid a race condition. [#290 Promote create_agent lock to top of function, to avoid race condition](https://github.com/webrtc-rs/webrtc/pull/290) contributed by [efer-ms](https://github.com/efer-ms)

## v0.5.0

### Changes

#### Breaking changes

* The serialized format for `RTCIceCandidateInit` has changed to match what the specification i.e. keys are camelCase. [#153 Make RTCIceCandidateInit conform to WebRTC spec](https://github.com/webrtc-rs/webrtc/pull/153) contributed by [jmatss](https://github.com/jmatss).
* Improved robustness when proposing RTP extension IDs and handling of collisions in these. This change is only breaking if you have assumed anything about the nature of these extension IDs. [#154 Fix RTP extension id collision](https://github.com/webrtc-rs/webrtc/pull/154) contributed by [k0nserv](https://github.com/k0nserv)
* Transceivers will now not stop when either or both directions are disabled. That is, applying and SDP with `a=inactive` will not stop the transceiver, instead attached senders and receivers will pause. A transceiver can be resurrected by setting direction back to e.g. `a=sendrecv`. The desired direction can be controlled with the newly introduced public method `set_direction` on `RTCRtpTransceiver`.
  * [#201 Handle inactive transceivers more correctly](https://github.com/webrtc-rs/webrtc/pull/201) contributed by [k0nserv](https://github.com/k0nserv)
  * [#210 Rework transceiver direction support further](https://github.com/webrtc-rs/webrtc/pull/210) contributed by [k0nserv](https://github.com/k0nserv)
  * [#214 set_direction add missing Send + Sync bound](https://github.com/webrtc-rs/webrtc/pull/214) contributed by [algesten](https://github.com/algesten)
  * [#213 set_direction add missing Sync bound](https://github.com/webrtc-rs/webrtc/pull/213) contributed by [algesten](https://github.com/algesten)
  * [#212 Public RTCRtpTransceiver::set_direction](https://github.com/webrtc-rs/webrtc/pull/212) contributed by [algesten](https://github.com/algesten)
  * [#268 Fix current direction update when applying answer](https://github.com/webrtc-rs/webrtc/pull/268) contributed by [k0nserv](https://github.com/k0nserv)
  * [#236 Pause RTP writing if direction indicates it](https://github.com/webrtc-rs/webrtc/pull/236) contributed by [algesten](https://github.com/algesten)
* Generated the `a=msid` line for `m=` line sections according to the specification. This might be break remote peers that relied on the previous, incorrect, behaviour. This also fixes a bug where an endless negotiation loop could happen. [#217 Correct msid handling for RtpSender](https://github.com/webrtc-rs/webrtc/pull/217) contributed by [k0nserv](https://github.com/k0nserv)
* Improve data channel id negotiation. We've slightly adjust the public interface for creating pre-negotiated data channels. Instead of a separate `negotiated: Option<bool>` and `id: Option<u16>` in `RTCDataChannelInit` there's now a more idiomatic `negotiated: Option<u16>`. If you have a pre-negotiated data channel simply set `negotiated: Some(id)` when creating the data channel.
  * [#237 Fix datachannel id setting for 0.5.0 release](https://github.com/webrtc-rs/webrtc/pull/237) contributed by [stuqdog](https://github.com/stuqdog)
  * [#229 Revert "base id updating on whether it's been negotiated, not on its …](https://github.com/webrtc-rs/webrtc/pull/229) contributed by [melekes](https://github.com/melekes)

  * [#226 base id updating on whether it's been finalized, not on its value](https://github.com/webrtc-rs/webrtc/pull/226) contributed by [stuqdog](https://github.com/stuqdog)


#### Other improvememnts

We made various improvements and fixes since 0.4.0, including merging all subcrates into a single git repo. The old crate repos are archived and all development will now happen in https://github.com/webrtc-rs/webrtc/.

* We now provide stats reporting via the standardized `RTCPeerConnection::get_stats` method.
  * [#277 Implement Remote Inbound Stats](https://github.com/webrtc-rs/webrtc/pull/277) contributed by [k0nserv](https://github.com/k0nserv)
  * [#220 Make stats types pub so they can be used directly](https://github.com/webrtc-rs/webrtc/pull/220) contributed by [k0nserv](https://github.com/k0nserv)
  * [#225 Add RTP Stats to stats report](https://github.com/webrtc-rs/webrtc/pull/225) contributed by [k0nserv](https://github.com/k0nserv)
  * [#189 Serialize stats](https://github.com/webrtc-rs/webrtc/pull/189) contributed by [sax](https://github.com/sax)
  * [#180 Get stats from peer connection](https://github.com/webrtc-rs/webrtc/pull/180) contributed by [sax](https://github.com/sax)

* [#278 Fix async-global-executor](https://github.com/webrtc-rs/webrtc/pull/278) contributed by [k0nserv](https://github.com/k0nserv)
* [#276 relax regex version requirement](https://github.com/webrtc-rs/webrtc/pull/276) contributed by [melekes](https://github.com/melekes)
* [#244 Update README.md instructions after monorepo merge](https://github.com/webrtc-rs/webrtc/pull/244) contributed by [k0nserv](https://github.com/k0nserv)
* [#241 move profile to workspace](https://github.com/webrtc-rs/webrtc/pull/241) contributed by [xnorpx](https://github.com/xnorpx)
* [#240 Increase timeout to "fix" test breaking](https://github.com/webrtc-rs/webrtc/pull/240) contributed by [algesten](https://github.com/algesten)
* [#239 One repo (again)](https://github.com/webrtc-rs/webrtc/pull/239) contributed by [algesten](https://github.com/algesten)
* [#234 Fix recent clippy lints](https://github.com/webrtc-rs/webrtc/pull/234) contributed by [k0nserv](https://github.com/k0nserv)
* [#224 update call to DataChannel::accept as per data pr #14](https://github.com/webrtc-rs/webrtc/pull/224) contributed by [melekes](https://github.com/melekes)
* [#223 dtls_transport: always set remote certificate](https://github.com/webrtc-rs/webrtc/pull/223) contributed by [melekes](https://github.com/melekes)
* [#216 Lower case mime types for comparison in fmpt lines](https://github.com/webrtc-rs/webrtc/pull/216) contributed by [k0nserv](https://github.com/k0nserv)
* [#211 Helper to trigger negotiation_needed](https://github.com/webrtc-rs/webrtc/pull/211) contributed by [algesten](https://github.com/algesten)
* [#209 MID generator feature](https://github.com/webrtc-rs/webrtc/pull/209) contributed by [algesten](https://github.com/algesten)
* [#208 update deps + loosen some requirements](https://github.com/webrtc-rs/webrtc/pull/208) contributed by [melekes](https://github.com/melekes)
* [#205 data_channel: handle stream EOF](https://github.com/webrtc-rs/webrtc/pull/205) contributed by [melekes](https://github.com/melekes)
* [#204 [peer_connection] allow persistent certificates](https://github.com/webrtc-rs/webrtc/pull/204) contributed by [melekes](https://github.com/melekes)
* [#202 bugfix-Udp connection not close (reopen #174) #195](https://github.com/webrtc-rs/webrtc/pull/202) contributed by [shiqifeng2000](https://github.com/shiqifeng2000)
* [#199 Upgrade ICE to 0.7.0](https://github.com/webrtc-rs/webrtc/pull/199) contributed by [k0nserv](https://github.com/k0nserv)
* [#194 Add AV1 MimeType and RtpCodecParameters](https://github.com/webrtc-rs/webrtc/pull/194) contributed by [billylindeman](https://github.com/billylindeman)
* [#188 Improve operations debuggability](https://github.com/webrtc-rs/webrtc/pull/188) contributed by [k0nserv](https://github.com/k0nserv)
* [#187 Fix SDP for rejected tracks to conform to RFC](https://github.com/webrtc-rs/webrtc/pull/187) contributed by [k0nserv](https://github.com/k0nserv)
* [#185 Adding some debug and display traits](https://github.com/webrtc-rs/webrtc/pull/185) contributed by [sevensidedmarble](https://github.com/sevensidedmarble)
* [#179 Fix example names in README](https://github.com/webrtc-rs/webrtc/pull/179) contributed by [ethagnawl](https://github.com/ethagnawl)
* [#176 Time overflow armv7 workaround](https://github.com/webrtc-rs/webrtc/pull/176) contributed by [frjol](https://github.com/frjol)
* [#171 close DTLS conn upon err](https://github.com/webrtc-rs/webrtc/pull/171) contributed by [melekes](https://github.com/melekes)
* [#170 always start sctp](https://github.com/webrtc-rs/webrtc/pull/170) contributed by [melekes](https://github.com/melekes)
* [#167 Add offer/answer/pranswer constructors for RTCSessionDescription](https://github.com/webrtc-rs/webrtc/pull/167) contributed by [sax](https://github.com/sax)

#### Subcrate updates

The various sub-crates have been updated as follows:

* util: 0.5.3 => 0.6.0
* sdp:  0.5.1 => 0.5.2
* mdns: 0.4.2 => 0.5.0
* stun: 0.4.2 => 0.4.3
* turn: 0.5.3 => 0.6.0
* ice: 0.6.4 => 0.8.0
* dtls: 0.5.2 => 0.6.0
* rtcp: 0.6.5 => 0.7.0
* rtp: 0.6.5 => 0.6.7
* srtp: 0.8.9 => 0.9.0
* scpt: 0.4.3 => 0.6.1
* data: 0.3.3 => 0.5.0
* interceptor: 0.7.6 => 0.8.0
* media: 0.4.5 => 0.4.7

Their respective change logs are found in the old, now archived, repositories and within their respective `CHANGELOG.md` files in the monorepo.

### Contributors

A big thanks to all the contributors that have made this release happen:

* [morajabi](https://github.com/morajabi)
* [sax](https://github.com/sax)
* [ethagnawl](https://github.com/ethagnawl)
* [xnorpx](https://github.com/xnorpx)
* [frjol](https://github.com/frjol)
* [algesten](https://github.com/algesten)
* [shiqifeng2000](https://github.com/shiqifeng2000)
* [billylindeman](https://github.com/billylindeman)
* [sevensidedmarble](https://github.com/sevensidedmarble)
* [k0nserv](https://github.com/k0nserv)
* [stuqdog](https://github.com/stuqdog)
* [neonphog](https://github.com/neonphog)
* [melekes](https://github.com/melekes)
* [jmatss](https://github.com/jmatss)


## Prior to 0.5.0

Before 0.5.0 there was no changelog, previous changes are sometimes, but not always, available in the [GitHub Releases](https://github.com/webrtc-rs/webrtc/releases).
