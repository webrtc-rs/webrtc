use std::net::Ipv4Addr;
use tokio::net::UdpSocket;

use util::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let socket = UdpSocket::bind("0.0.0.0:8888").await?;
    let mut buf = [0u8; 65535];
    let multi_addr = Ipv4Addr::new(224, 0, 0, 251);
    let inter = Ipv4Addr::new(0, 0, 0, 0);
    socket.join_multicast_v4(multi_addr, inter)?;
    println!("listening@{:?}...", socket.local_addr());

    loop {
        let (amt, src) = socket.recv_from(&mut buf).await?;
        println!("received {} bytes from {:?}", amt, src);
    }
}
