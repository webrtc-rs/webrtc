use clap::{App, Arg};

use stun::message::Message;

fn main() {
    let mut app = App::new("STUN decode")
        .version("0.1.0")
        .author("Jtplouffe <jtplouffe@gmail.com>")
        .about("An example of STUN decode")
        .arg(
            Arg::with_name("FULLHELP")
                .help("Prints more detailed help information")
                .long("fullhelp"),
        )
        .arg(
            Arg::with_name("data")
                .required_unless("FULLHELP")
                .takes_value(true)
                .index(1)
                .help("base64 encoded message, e.g. 'AAEAHCESpEJML0JTQWsyVXkwcmGALwAWaHR0cDovL2xvY2FsaG9zdDozMDAwLwAA'"),
        );

    let matches = app.clone().get_matches();

    if matches.is_present("FULLHELP") {
        app.print_long_help().unwrap();
        std::process::exit(0);
    }

    let encoded_data = matches.value_of("data").unwrap();
    let decoded_data = match base64::decode(encoded_data) {
        Ok(d) => d,
        Err(e) => panic!("Unable to decode base64 value: {e}"),
    };

    let mut message = Message::new();
    message.raw = decoded_data;

    match message.decode() {
        Ok(_) => println!("{message}"),
        Err(e) => panic!("Unable to decode message: {e}"),
    }
}
