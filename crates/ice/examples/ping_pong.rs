//use webrtc_ice as ice;

use clap::{App, AppSettings, Arg};

use util::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();

    let mut app = App::new("ICE Demo")
        .version("0.1.0")
        .author("Rain Liu <yliu@webrtc.rs>")
        .about("An example of ICE")
        .setting(AppSettings::DeriveDisplayOrder)
        .setting(AppSettings::SubcommandsNegateReqs)
        .arg(
            Arg::with_name("FULLHELP")
                .help("Prints more detailed help information")
                .long("fullhelp"),
        )
        .arg(
            Arg::with_name("controlling")
                .required_unless("FULLHELP")
                .takes_value(false)
                .long("controlling")
                .help("is ICE Agent controlling"),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let _is_controlling = matches.is_present("controlling");

    Ok(())
}
