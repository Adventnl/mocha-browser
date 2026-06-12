# mocha_net test data

`localhost-cert.der` / `localhost-key.der` are a **test-only** self-signed
certificate and PKCS#8 private key (EC P-256, CN=localhost, SAN
`DNS:localhost, IP:127.0.0.1`, valid ~100 years). They exist so the localhost
TLS test server (`test_server::TestServer::start_tls`) can accept real rustls
handshakes in integration tests without touching the network.

The private key is intentionally public. It must never be trusted outside
tests: the production client trusts only the embedded webpki root store, and
tests opt in to this certificate explicitly via
`DefaultLoader::with_extra_tls_root`.

Regenerate with:

```sh
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 \
  -keyout key.pem -out cert.pem -days 36500 -nodes -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"
openssl x509 -in cert.pem -outform der -out localhost-cert.der
openssl pkcs8 -topk8 -nocrypt -in key.pem -outform der -out localhost-key.der
```
