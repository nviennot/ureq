# ureq Future Direction
January 2020

## Background
Thought that occurred in December 2017:

> This library tries to provide a convenient request library with a minimal dependency tree and an obvious API.

The motivation to write ureq came from a mindset I don't like in myself. It was an anti-reaction to [reqwest](https://docs.rs/reqwest/0.10.1/reqwest/). I don't really want to be the _anti-something_ person. However to motivate the existence of yet another http client library, it's worth exploring differences.

I don't mean any of this as criticism as I am totally in awe of the people behind reqwest, hyper, h2 and tokio (and all other fantastic libraries they make). My coding skills don't even compare to theirs. Any future direction ureq might take, will most certainly share a ton of code with reqwest by using the same underlying libraries.

Speaking of code. The ureq code in master makes me blush – early sins while learning Rust. I've improved my game a bit since then.

## User centric
ureq came from a feeling of a need of a user centric (or maybe "ergonomic"?) library.  [SuperAgent](https://visionmedia.github.io/superagent/) was a big inspiration in terms of easy and obvious API. This is not to say reqwest is not easy to use, it's fine. However faced with a compromise between easy and performant API, how far towards "easy" does it go?

Hyper, which largely underpins reqwest, has a stated primary goal of "A **fast** and **correct** HTTP implementation for Rust." (their emphasis).  I think this sometimes "leaks" through to the user.

A library with a clear non-compromising "user first" idea might still be a good idea. Treating user input as "just make it work" instead of enforcing correctness.

## Dependency tree size
At the time ureq came about, rust did not have `async` and `await`. reqwest exposed a blocking API , but the underpinnings were all using pre-`async` tokio futures.

That old tokio had a hefty dependency tree. I remember feeling that for a small micro service doing some simple http requests to S3, the amount of compilation time needed was just too much. However tokio is better today. A quick check with bash and `cargo tree` using default features, shows it moved from ~100 in December 2017 to ~70 today.

The amount of dependencies in a library is a divisive topic. I'm not rabid anti-dependencies, crates.io and cargo is amazing, and all my work builds on what other developers have made.

But I was burnt. NodeJS came as a breath of fresh air compared to what was before. Then it went downhills into babel compilation, webpack and [unit test runners needing 240 other packages](http://npm.broofa.com/?q=ava).

There's a middle ground somewhere and that's where I want to be.

## unsafe
The use of `unsafe` is another  divisive topic. I am not against unsafe, but I don't trust myself with it either. ureq never set out to be against `unsafe` and then ironically got some recognition for not having much unsafe code.

With hindsight I had no thoughts about `unsafe` back then. ureq ended up not having much `unsafe` cause I was a total Rust noob.

Is there a case to make an http client library that differentiates on use of unsafe? Judging by the intensity of the anti-unsafe crowd on Reddit, probably yes. Do I want to be part of that crowd? No.

However in a high level library, it's pretty easy to avoid `unsafe` if we accept that we're not optimising for exceptional performance (allow ourselves the odd extra allocation, or `Box<dyn…>)`.

Dependencies is another matter though. Especially an `async` ureq would need to rely more on non-std libraries which in turn means more `unsafe`. I consider async-std, tokio, mio etc to be perfectly fine, but someone counting `unsafe` keywords using code scanning tools probably wouldn't.

## What next?
Should this library continue? Is this space maybe better served by others?

### http crate
I'm excited about exploring an [http::Request](https://docs.rs/http/0.2.0/http/request/index.html) based API all about "user first".  How much can be achieved using only trait extensions `RequestBuilderExt`, `RequestExt` ?

Is it helpful to provide ways around super correct types like [HeaderValue](https://docs.rs/http/0.2.0/http/header/struct.HeaderValue.html)?

What about query parameters?

```rust
let response = http::Request::builder()
    .get("https://my-service")
    .query("doc-id", my_doc_id)
    .build(())?;
```

How far can I avoid complicated life times, trait bounds and magical traits in favour of "simple" well knowns types like  `&str`, `&[u8]`. Is there  point in doing so?

### Single threaded schedulers
A simple micro service doing some S3 requests doesn't necessarily need to fire up a multi threaded, work stealing async runtime.

The simplest possible async executors are only about [100 LOC](https://github.com/richardanaya/executor/blob/master/src/lib.rs). But to make a request client library, we need the executor to be connected to an async I/O layer (like [mio](https://crates.io/crates/mio)). That layer makes futures wake up when data arrives on my TcpStream. I believe this is the distinction between an async _executor_ and an async _runtime_.

For runtimes _with I/O_ I've only found two choices, [async-std](https://crates.io/crates/async-std) and [tokio](https://crates.io/crates/tokio) (are there more?).

tokio has a feature `rt-core` which is a "single threaded scheduler".  If we imagine an API like:

```rust
let response = http::Request::builder()
    .get("https://my-service")
    .query("doc-id", my_doc_id)
    .send().block()?;
```

With signature:
`async fn send() -> Result<http::Response<Body>, Error>`

`send` returns a Future.  If we're not running in an async context (and therefore could use `.await`), we could provide a `block()` that simply "borrows" the current executing thread, run the underlying async code and return the result.

I'm excited about the possibilities of minimal single threaded runtimes that keeps the dependencies and complexity down. Maybe write one myself?

If I code only against generic `AsyncRead` and `AsyncWrite`, I could make a ureq with extremely simple defaults and still pluggable into more complex runtimes when available.

### Exploration
I've spent the week in frantic coding mode and there's  a new branch where I explore possibilities. It still might come to nothing and the project is parked.

https://github.com/algesten/ureq/tree/future

Thoughts and feedback are very welcome. Use the issue tracker if you want.
