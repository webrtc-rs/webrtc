#![warn(rust_2018_idioms)]
#![allow(dead_code)]

//! Package rtcp implements encoding and decoding of RTCP packets according to RFCs 3550 and 5506.
//!
//! RTCP is a sister protocol of the Real-time Transport Protocol (RTP). Its basic functionality
//! and packet structure is defined in RFC 3550. RTCP provides out-of-band statistics and control
//! information for an RTP session. It partners with RTP in the delivery and packaging of multimedia data,
//! but does not transport any media data itself.
//!
//! The primary function of RTCP is to provide feedback on the quality of service (QoS)
//! in media distribution by periodically sending statistics information such as transmitted octet
//! and packet counts, packet loss, packet delay variation, and round-trip delay time to participants
//! in a streaming multimedia session. An application may use this information to control quality of
//! service parameters, perhaps by limiting flow, or using a different codec.
//!
//! Decoding RTCP packets:
//!```nobuild
//!     let pkt = rtcp::unmarshal(&rtcp_data).unwrap();
//!
//!     if let Some(e) = pkt
//!          .as_any()
//!          .downcast_ref::<PictureLossIndication>()
//!      {
//!    
//!      }
//!     else if let Some(e) = packet
//!          .as_any()
//!          .downcast_ref::<Goodbye>(){}
//!     ....
//!```
//!
//! Encoding RTCP packets:
//!```nobuild
//!     let pkt = PictureLossIndication{
//!         sender_ssrc: sender_ssrc,
//!         media_ssrc: media_ssrc
//!     };
//!
//!     let pli_data = pkt.marshal().unwrap();
//!     // ...
//!```

pub mod compound_packet;
mod error;
pub mod extended_report;
pub mod goodbye;
pub mod header;
pub mod packet;
pub mod payload_feedbacks;
pub mod raw_packet;
pub mod receiver_report;
pub mod reception_report;
pub mod sender_report;
pub mod source_description;
pub mod transport_feedbacks;
mod util;

pub use error::Error;
