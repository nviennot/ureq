# ureq

![](https://github.com/algesten/ureq/workflows/CI/badge.svg)

# FUTURE DIRECTION BRANCH. NOT USEFUL

What are we doing?

* [x] Parse URL (http)
* [x] Set request headers (http)
  * [ ] Username/password
  * [ ] authorization header
* [x] Request/response body
* [x] Resolve DNS (dns-lookup)
* [ ] Timeout for entire request.
* [x] Connect socket … or is this API surface?
* [x] Wrap socket in SSL (tls-api)
* [x] Talk http1 (write own h1, httparse)
* [x] Talk http2 (h2)
* [ ] Ergonomic body methods
* Body data transformations
  * [x] chunked encoding (my own)
  * [ ] x-www-form-urlencoded (write it?)
  * [ ] form-data (multipart) (write it?)
* [x] Query parameters
* Content decoder
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

> Minimal request library in rust.

## Usage

TODO

## Features

TODO

## Motivation

  * Minimal dependency tree
  * Obvious API
  * Convencience over correctness

This library tries to provide a convenient request library with a minimal dependency
tree and an obvious API. It is inspired by libraries like
[superagent](http://visionmedia.github.io/superagent/) and
[fetch API](https://developer.mozilla.org/en-US/docs/Web/API/Fetch_API).

This library does not try to enforce web standards correctness. It uses HTTP/1.1,
but whether the request is _perfect_ HTTP/1.1 compatible is up to the user of the
library. For example:

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
