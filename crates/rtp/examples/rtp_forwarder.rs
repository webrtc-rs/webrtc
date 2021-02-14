use std::process::{Command, Stdio};
use std::{net::UdpSocket, sync::mpsc, thread::sleep};
use webrtc_rs_rtp::codecs::h264::*;
use webrtc_rs_rtp::packetizer::{new_packetizer, PacketizerInterface};
use webrtc_rs_rtp::sequence;

fn main() {
    let path = std::env::current_dir().unwrap();
    println!("{:?}", path);

    let mut packetizer = new_packetizer(
        1200,
        96,
        0x1234ABCD,
        90000,
        Box::new(H264Payloader),
        Box::new(sequence::new_random_sequencer()),
    );

    let socket = UdpSocket::bind("127.0.0.1:1235").unwrap();

    let (send_channel, recv_channel) = mpsc::channel();
    let mut index = 0;

    std::thread::spawn(move || {
        println!("starting");
        while let Ok(e) = recv_channel.recv() {
            let payload: Vec<u8> = e;
            socket.send_to(&payload, "127.0.0.1:5004").unwrap();
        }
        println!("exited")
    });

    loop {
        let cmd = Command::new(path.join("ffmpeg"))
            .args(&[
                "-i",
                "input.mp4",
                "-ss",
                &format!("{}", index),
                "-t",
                "0.1",
                "-c:v",
                "h264",
                "-f",
                "h264",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .unwrap();

        let mut a = cmd.stdout;
        let sender = send_channel.clone();
        let packets = packetizer.packetize(a.as_mut_slice(), index).unwrap();

        std::thread::spawn(move || {
            println!("{}", packets.len());

            for mut packet in packets {
                let mut payload = packet.header.marshal().unwrap();
                payload.extend_from_slice(&packet.payload);

                let c = &payload[..];
                // println!("{:02X?}", payload);
                // exit(0);
                sender.send(payload).unwrap();
            }
        });

        index += 1;
        sleep(std::time::Duration::from_millis(1500));
    }
}
