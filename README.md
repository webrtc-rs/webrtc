<h1 align="center">
 <a href="https://webrtc.rs"><img src="./doc/webrtc.rs.png" alt="WebRTC.rs"></a>
 <br>
</h1>
<p align="center">
 <a href="https://github.com/webrtc-rs/webrtc/actions"> 
  <img src="https://github.com/webrtc-rs/webrtc/workflows/webrtc/badge.svg?branch=master">
 </a> 
 <a href="https://codecov.io/gh/webrtc-rs/webrtc"> 
  <img src="https://codecov.io/gh/webrtc-rs/webrtc/branch/master/graph/badge.svg">
 </a>
 <a href="https://deps.rs/repo/github/webrtc-rs/webrtc"> 
  <img src="https://deps.rs/repo/github/webrtc-rs/webrtc/status.svg">
 </a>
 <a href="https://crates.io/crates/webrtc"> 
  <img src="https://img.shields.io/crates/v/webrtc.svg">
 </a> 
 <a href="https://docs.rs/webrtc"> 
  <img src="https://docs.rs/webrtc/badge.svg">
 </a>
 <a href="https://github.com/webrtc-rs/webrtc/blob/master/LICENSE">
  <img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT">
 </a>
 <a href="https://discord.gg/4Ju8UHdXMs">
  <img src="https://img.shields.io/discord/800204819540869120?logo=discord" alt="Discord">
 </a>
 <a href="https://twitter.com/WebRTCrs">
  <img src="https://img.shields.io/twitter/url/https/twitter.com/webrtcrs.svg?style=social&label=%40WebRTCrs" alt="Twitter">
 </a>  
</p>
<p align="center">
 A pure Rust implementation of WebRTC stack. Rewrite <a href="http://Pion.ly">Pion</a> WebRTC stack in Rust
</p>

#

[webrtc-rs]: ./doc/webrtc.rs.png
[webrtc-stack]: ./doc/webrtc_stack.png
[check]: ./doc/check.png
[uncheck]: ./doc/uncheck.png
[util-badge]: https://img.shields.io/crates/v/webrtc-util.svg
[util-url]: https://crates.io/crates/webrtc-util
[sdp-badge]: https://img.shields.io/crates/v/sdp.svg
[sdp-url]: https://crates.io/crates/sdp
[rtp-badge]: https://img.shields.io/crates/v/rtp.svg
[rtp-url]: https://crates.io/crates/rtp
[rtcp-badge]: https://img.shields.io/crates/v/rtcp.svg
[rtcp-url]: https://crates.io/crates/rtcp
[srtp-badge]: https://img.shields.io/crates/v/webrtc-srtp.svg
[srtp-url]: https://crates.io/crates/webrtc-srtp
[dtls-badge]: https://img.shields.io/crates/v/webrtc-dtls.svg
[dtls-url]: https://crates.io/crates/webrtc-dtls
[stun-badge]: https://img.shields.io/crates/v/stun.svg
[stun-url]: https://crates.io/crates/stun
[mdns-badge]: https://img.shields.io/crates/v/webrtc-mdns.svg
[mdns-url]: https://crates.io/crates/webrtc-mdns
[ice-badge]: https://img.shields.io/crates/v/webrtc-ice.svg
[ice-url]: https://crates.io/crates/webrtc-ice
[turn-badge]: https://img.shields.io/crates/v/turn.svg
[turn-url]: https://crates.io/crates/turn
[sctp-badge]: https://img.shields.io/crates/v/webrtc-sctp.svg
[sctp-url]: https://crates.io/crates/webrtc-sctp
[sip-badge]: https://img.shields.io/crates/v/webrtc-sip.svg
[sip-url]: https://crates.io/crates/webrtc-sip
[pc-badge]: https://img.shields.io/crates/v/webrtc-pc.svg
[pc-url]: https://crates.io/crates/webrtc-pc
[data-badge]: https://img.shields.io/crates/v/webrtc-data.svg
[data-url]: https://crates.io/crates/webrtc-data
[media-badge]: https://img.shields.io/crates/v/webrtc-media.svg
[media-url]: https://crates.io/crates/webrtc-media
[rtc-badge]: https://img.shields.io/crates/v/rtc.svg
[rtc-url]: https://crates.io/crates/rtc

<div style="text-align: center;">

![uncheck][uncheck]RTC[![rtc][rtc-badge]][rtc-url]
![uncheck][uncheck]Media[![media][media-badge]][media-url]
![uncheck][uncheck]PeerConnection[![pc][pc-badge]][pc-url]
![uncheck][uncheck]DataChannel[![data][data-badge]][data-url]

![check][check]RTP[![rtp][rtp-badge]][rtp-url] 
![check][check]RTCP[![rtcp][rtcp-badge]][rtcp-url]
![check][check]SRTP[![srtp][srtp-badge]][srtp-url]
![uncheck][uncheck]SCTP[![sctp][sctp-badge]][sctp-url]

![check][check]DTLS[![dtls][dtls-badge]][dtls-url]

![check][check]ICE[![ice][ice-badge]][ice-url]
![check][check]STUN[![stun][stun-badge]][stun-url]
![check][check]TURN[![turn][turn-badge]][turn-url]
![check][check]mDNS[![mdns][mdns-badge]][mdns-url]
 
![check][check]SDP[![sdp][sdp-badge]][sdp-url]
![check][check]Util[![sdp][util-badge]][util-url] 

</div>

#

<h1 align="center">
 <img src="./doc/webrtc_stack.png" alt="WebRTC Stack">
</h1>

<p align="center">
<strong>Sponsored with 💖 by</strong><br>
</p>
<p align="center">
<a href="https://getstream.io/?utm_source=github.com/webrtc-rs/webrtc&utm_medium=github&utm_campaign=oss_sponsorship" target="_blank">
<img src="https://stream-blog-v2.imgix.net/blog/wp-content/uploads/f7401112f41742c4e173c30d4f318cb8/stream_logo_white.png?h=50" alt="Stream Chat">
</a> <a href="https://www.embark-studios.com/" target="_blank"><img src="./doc/embark.jpg" alt="embark"></a>
</p>


