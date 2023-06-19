use std::error::Error;

use webrtc_util::ifaces::ifaces;

fn main() -> Result<(), Box<dyn Error>> {
    let interfaces = ifaces()?;
    for (index, interface) in interfaces.iter().enumerate() {
        println!("{index} {interface:?}");
    }
    Ok(())
}
