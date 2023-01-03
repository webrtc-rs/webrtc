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

in pion/examples/util/util.go, convert ECPrivateKey to PKCS8PrivateKey
```go
func LoadKey(path string) (crypto.PrivateKey, error) {
    ....
    if key, err := x509.ParseECPrivateKey(block.Bytes); err == nil {
		b, err := x509.MarshalPKCS8PrivateKey(key)
		if err != nil {
			return nil, err
		}
		var pemPrivateBlock = &pem.Block{
			Type:  "PRIVATE KEY",
			Bytes: b,
		}
		pemPrivateFile, err := os.Create(path+".private_key.pem")
		if err != nil {
			fmt.Println(err)
			os.Exit(1)
		}
		err = pem.Encode(pemPrivateFile, pemPrivateBlock)
		if err != nil {
			fmt.Println(err)
			os.Exit(1)
		}
		pemPrivateFile.Close()

		return key, nil
	}
	...
```