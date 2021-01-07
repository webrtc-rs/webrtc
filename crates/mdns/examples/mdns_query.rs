use tokio::net::UdpSocket;
use tokio::time;

use tokio::time::Duration;
use util::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let buf = [1u8; 15000];
    let count = 1473;
    socket.send_to(&buf[0..count], "224.0.0.251:8888").await?;
    println!("sending...");
    time::sleep(Duration::from_secs(1)).await;
    println!("done");
    Ok(())
}
