# ureq

![](https://github.com/algesten/ureq/workflows/CI/badge.svg)

# FUTURE DIRECTION BRANCH. NOT USEFUL YET

[Thoughts here](THOUGHTS.md)

What are we doing?

* [x] Parse URL (http)
* [x] Set request headers (http)
* [x] Request/response body
* [x] Resolve DNS (dns-lookup)
* [x] Timeout for entire request.
  * [ ] Propagate timeout to socket itself
* [x] Connect socket … or is this API surface?
* [x] Wrap socket in SSL (tls-api)
* [x] Talk http1 (write own h1, httparse)
* [x] Talk http2 (h2)
* [ ] Sniff default tokio runtime?
* [ ] Set tokio Runtime
* [x] Explore http::Request extension mechanic
* Ergonomic RequestExt
  * [x] Query parameters
  * [ ] Username/password
  * [ ] authorization header
  * [ ] Serialize JSON
* Ergonomic body
  * [x] AsyncRead
  * [x] Read to Vec
  * [x] Read to String
  * [ ] Read to JSON
* Body data transformations
  * [x] chunked encoding (my own)
  * [ ] x-www-form-urlencoded (write it?)
  * [ ] form-data (multipart) (write it?)
* Content decoding
  * [x] character sets
  * [x] gzip
* Content encoding
  * [ ] character sets
  * [x] gzip
* [ ] Retry logic
* [ ] Connection pooling
* [ ] Cookie state in connection (cookie)
* [ ] HTTP Proxy
* [ ] Follow redirects
* [ ] expect-100
* [ ] Upstream PassTlsConnector
* [ ] Cleanup Errors (implement Display proper)
* [ ] Tests
* [ ] Doc

> Minimal request library in rust.

## Motivation

  * Obvious API
  * Convencience over correctness
  * Minimal dependency tree

This library tries to provide a convenient request library with a minimal dependency
tree and an obvious API. It is inspired by libraries like
[superagent](http://visionmedia.github.io/superagent/) and
[fetch API](https://developer.mozilla.org/en-US/docs/Web/API/Fetch_API).

## License

Copyright (c) 2019 Martin Algesten

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
