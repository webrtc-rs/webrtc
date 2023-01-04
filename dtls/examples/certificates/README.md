# Certificates

The certificates in for the examples are generated using the commands shown below.

Note that this was run on OpenSSL 1.1.1d, of which the arguments can be found in the [OpenSSL Manpages](https://www.openssl.org/docs/man1.1.1/man1), and is not guaranteed to work on different OpenSSL versions.

```shell
# Extensions required for certificate validation.
$ EXTFILE='extfile.conf'
$ echo 'subjectAltName = DNS:webrtc.rs' > "${EXTFILE}"

# Server.
$ SERVER_NAME='server'
$ openssl ecparam -name prime256v1 -genkey -noout -out "${SERVER_NAME}.pem"
$ openssl req -key "${SERVER_NAME}.pem" -new -sha256 -subj '/C=NL' -out "${SERVER_NAME}.csr"
$ openssl x509 -req -in "${SERVER_NAME}.csr" -extfile "${EXTFILE}" -days 365 -signkey "${SERVER_NAME}.pem" -sha256 -out "${SERVER_NAME}.pub.pem"

# Client.
$ CLIENT_NAME='client'
$ openssl ecparam -name prime256v1 -genkey -noout -out "${CLIENT_NAME}.pem"
$ openssl req -key "${CLIENT_NAME}.pem" -new -sha256 -subj '/C=NL' -out "${CLIENT_NAME}.csr"
$ openssl x509 -req -in "${CLIENT_NAME}.csr" -extfile "${EXTFILE}" -days 365 -CA "${SERVER_NAME}.pub.pem" -CAkey "${SERVER_NAME}.pem" -set_serial '0xabcd' -sha256 -out "${CLIENT_NAME}.pub.pem"

# Cleanup.
$ rm "${EXTFILE}" "${SERVER_NAME}.csr" "${CLIENT_NAME}.csr"
```

## Converting EC private key to PKCS#8 in Rust

`Cargo.toml`:

```toml
[dependencies]
topk8 = "0.0.1"
```

`main.rs`:

```rust
fn main() {
    let ec_pem = "
-----BEGIN EC PRIVATE KEY-----
MHcCAQEEIAL4r6d9lPq3XEDSZTL9l0D6thrPM7RiAhl3Fjuw9Ji2oAoGCCqGSM49
AwEHoUQDQgAE4U64dviQRMujGK0g80dwzgjV7fnwLkj6RfvINMHvD6eiCsphWIlq
cddTAoOjXVQDu3qMAS1Ghfyk1F377EW1Sw==
-----END EC PRIVATE KEY-----
";

    let pkcs8_pem = topk8::from_sec1_pem(ec_pem).unwrap();

    println!("{}", pkcs8_pem);

    // -----BEGIN PRIVATE KEY-----
    // MIGTAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBHkwdwIBAQQgAvivp32U+rdcQNJl
    // Mv2XQPq2Gs8ztGICGXcWO7D0mLagCgYIKoZIzj0DAQehRANCAAThTrh2+JBEy6MY
    // rSDzR3DOCNXt+fAuSPpF+8g0we8Pp6IKymFYiWpx11MCg6NdVAO7eowBLUaF/KTU
    // XfvsRbVL
    // -----END PRIVATE KEY-----
}
```
