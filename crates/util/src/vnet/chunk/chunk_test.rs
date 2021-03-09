use super::*;

#[test]
fn test_tcp_frag_string() {
    let f = TCP_FLAG_FIN;
    assert_eq!("FIN", f.to_string(), "should match");
    let f = TCP_FLAG_SYN;
    assert_eq!("SYN", f.to_string(), "should match");
    let f = TCP_FLAG_RST;
    assert_eq!("RST", f.to_string(), "should match");
    let f = TCP_FLAG_PSH;
    assert_eq!("PSH", f.to_string(), "should match");
    let f = TCP_FLAG_ACK;
    assert_eq!("ACK", f.to_string(), "should match");
    let f = TCP_FLAG_SYN | TCP_FLAG_ACK;
    assert_eq!("SYN-ACK", f.to_string(), "should match");
}
