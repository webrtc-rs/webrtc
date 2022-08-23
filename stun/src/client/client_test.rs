use super::*;

#[test]
fn ensure_client_settings_is_send() {
    let client = ClientSettings::default();

    ensure_send(client);
}

fn ensure_send<T: Send>(_: T) {}

//TODO: add more client tests
