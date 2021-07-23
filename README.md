# Standalone Rendezvous Server

A standalone libp2p [rendezvous server](https://github.com/libp2p/specs/tree/master/rendezvous) binary.

## Usage

Run the `rendezvous_server`:

```
rendezvous_server --secret-file <PATH-TO-SECRET-FILE> --port 8888
```

Run `rendezvous_server --help` for more options

### TLS configuration

You can test with self signed certificates using `openssl`:

1. Create certificate:

```bash
# generate pass key to be used for generating private key
openssl genrsa -aes256 -passout pass:gsahdg -out server.pass.key 4096
# generate private key
openssl rsa -passin pass:gsahdg -in server.pass.key -out server.key
# remove pass key
rm server.pass.key

# create certificate signing request
openssl req -new -key server.key -out server.csr

# create self signed certificate from private key and signing request
openssl x509 -req -sha256 -days 365 -in server.csr -signkey server.key -out server.crt
```

2. Convert certificate to `der` format to be compatible with libp2p's TlsConfig:

```bash
# convert private key to der
openssl rsa -inform pem -in server.key -outform der -out server_pk.der
# convert certificate to der
openssl x509 -inform pem -in server.crt -outform der -out server_cert.der
```
