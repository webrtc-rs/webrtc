use std::io::Cursor;

use criterion::measurement::WallTime;
use criterion::{criterion_main, BenchmarkGroup, Criterion};
use sdp::SessionDescription;

const CANONICAL_UNMARSHAL_SDP: &str = "v=0\r\n\
o=jdoe 2890844526 2890842807 IN IP4 10.47.16.5\r\n\
s=SDP Seminar\r\n\
i=A Seminar on the session description protocol\r\n\
u=http://www.example.com/seminars/sdp.pdf\r\n\
e=j.doe@example.com (Jane Doe)\r\n\
p=+1 617 555-6011\r\n\
c=IN IP4 224.2.17.12/127\r\n\
b=X-YZ:128\r\n\
b=AS:12345\r\n\
t=2873397496 2873404696\r\n\
t=3034423619 3042462419\r\n\
r=604800 3600 0 90000\r\n\
z=2882844526 -3600 2898848070 0\r\n\
k=prompt\r\n\
a=candidate:0 1 UDP 2113667327 203.0.113.1 54400 typ host\r\n\
a=recvonly\r\n\
m=audio 49170 RTP/AVP 0\r\n\
i=Vivamus a posuere nisl\r\n\
c=IN IP4 203.0.113.1\r\n\
b=X-YZ:128\r\n\
k=prompt\r\n\
a=sendrecv\r\n\
m=video 51372 RTP/AVP 99\r\n\
a=rtpmap:99 h263-1998/90000\r\n";

fn benchmark_sdp(g: &mut BenchmarkGroup<WallTime>) {
    let mut reader = Cursor::new(CANONICAL_UNMARSHAL_SDP.as_bytes());
    let sdp = SessionDescription::unmarshal(&mut reader).unwrap();

    ///////////////////////////////////////////////////////////////////////////////////////////////

    // BenchmarkMarshal
    g.bench_function("Marshal", |b| {
        b.iter(|| {
            let _ = sdp.marshal();
        })
    });

    // BenchmarkUnmarshal
    g.bench_function("Unmarshal", |b| {
        b.iter(|| {
            let mut reader = Cursor::new(CANONICAL_UNMARSHAL_SDP.as_bytes());
            let _ = SessionDescription::unmarshal(&mut reader).unwrap();
        })
    });
}

fn benches() {
    let mut c = Criterion::default().configure_from_args();
    let mut g = c.benchmark_group("SDP");

    benchmark_sdp(&mut g);

    g.finish();
}

criterion_main!(benches);
