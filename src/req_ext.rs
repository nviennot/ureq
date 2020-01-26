use crate::deadline::Deadline;
use crate::res_ext::HeaderMapExt;
use crate::Body;
use async_trait::async_trait;
use http::request;
use http::Uri;
use http::{Request, Response};
use once_cell::sync::Lazy;
use qstring::QString;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub trait RequestBuilderExt
where
    Self: Sized,
{
    fn query(self, key: &str, value: &str) -> Self;
    fn timeout(self, duration: Duration) -> Self;
    fn from_body<B: Into<Body>>(self, body: B) -> http::Result<Request<Body>>;
}

impl RequestBuilderExt for request::Builder {
    //
    fn query(mut self, key: &str, value: &str) -> Self {
        with_builder_store(self.headers_mut(), |store| {
            store.query_params.push((key.into(), value.into()));
        });
        self
    }

    fn timeout(mut self, duration: Duration) -> Self {
        with_builder_store(self.headers_mut(), |store| {
            store.req_params.timeout = Some(duration);
        });
        self
    }

    fn from_body<B: Into<Body>>(self, body: B) -> http::Result<Request<Body>> {
        self.body(body.into())
    }
}

#[async_trait]
pub trait RequestExt {
    //

    /// Get a header, ignore incorrect header values.
    fn header(&self, key: &str) -> Option<&str>;

    fn header_as<T: FromStr>(&self, key: &str) -> Option<T>;

    /// Signature: `async fn send(self) -> Response<Body>`
    async fn send(self) -> Response<Body>;
}

#[async_trait]
impl<B: Into<Body> + Send> RequestExt for Request<B> {
    //
    fn header(&self, key: &str) -> Option<&str> {
        self.headers().get_str(key)
    }

    fn header_as<T: FromStr>(&self, key: &str) -> Option<T> {
        self.headers().get_as(key)
    }

    async fn send(self) -> Response<Body> {
        //
        unimplemented!()
    }
}

static ID_COUNTER: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));
static BUILDER_STORE: Lazy<Mutex<HashMap<usize, BuilderStore>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

struct BuilderStore {
    query_params: Vec<(String, String)>,
    req_params: RequestParams,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RequestParams {
    pub req_start: Option<Instant>,
    pub timeout: Option<Duration>,
}

impl RequestParams {
    pub fn new() -> Self {
        RequestParams {
            ..Default::default()
        }
    }

    pub fn deadline(&self) -> Deadline {
        Deadline::new(self.req_start, self.timeout)
    }
}

impl BuilderStore {
    fn new() -> Self {
        BuilderStore {
            query_params: vec![],
            req_params: RequestParams::new(),
        }
    }

    fn invoke(self, parts: &mut http::request::Parts) -> RequestParams {
        let mut uri_parts = parts.uri.clone().into_parts();

        // Construct new instance of PathAndQuery with our modified query.
        let new_path_and_query = {
            //
            let (path, query) = uri_parts
                .path_and_query
                .as_ref()
                .map(|p| (p.path(), p.query().unwrap_or("")))
                .unwrap_or(("", ""));

            let mut qs = QString::from(query);
            for (key, value) in self.query_params.into_iter() {
                qs.add_pair((key, value));
            }

            // PathAndQuery has no API for modifying any fields. This seems to be our only
            // option to get a new instance of it using the public API.
            let tmp: Uri = format!("http://fake{}?{}", path, qs).parse().unwrap();
            let tmp_parts = tmp.into_parts();
            tmp_parts.path_and_query.unwrap()
        };

        // This is good. We can change the PathAndQuery field.
        uri_parts.path_and_query = Some(new_path_and_query);
        let new_uri = Uri::from_parts(uri_parts).unwrap();

        parts.uri = new_uri;

        self.req_params
    }
}

fn with_builder_store<F: FnOnce(&mut BuilderStore)>(
    headers: Option<&mut http::HeaderMap<http::HeaderValue>>,
    f: F,
) {
    if let Some(headers) = headers {
        let val = headers
            .entry("x-ureq-ext")
            .or_insert_with(|| ID_COUNTER.fetch_add(1, Ordering::Relaxed).into());
        let id = val.to_str().unwrap().parse::<usize>().unwrap();
        let mut lock = BUILDER_STORE.lock().unwrap();
        let ureq_ext = lock.entry(id).or_insert_with(BuilderStore::new);
        f(ureq_ext);
    }
}

pub fn resolve_ureq_ext(parts: &mut http::request::Parts) -> Option<RequestParams> {
    if let Some(val) = parts.headers.remove("x-ureq-ext") {
        let id = val.to_str().unwrap().parse::<usize>().unwrap();
        let mut lock = BUILDER_STORE.lock().unwrap();
        if let Some(store) = lock.remove(&id) {
            let req_params = store.invoke(parts);
            return Some(req_params);
        }
    }
    None
}
