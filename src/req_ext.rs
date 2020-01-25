use crate::Body;
use async_trait::async_trait;
use http::request;
use http::Uri;
use http::{Request, Response};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use qstring::QString;

pub trait RequestBuilderExt
where
    Self: Sized,
{
    fn query(self, key: &str, value: &str) -> Self;
    fn from_body<B: Into<Body>>(self, body: B) -> http::Result<Request<Body>>;
}

impl RequestBuilderExt for request::Builder {
    //
    fn query(mut self, key: &str, value: &str) -> Self {
        with_ureq_ext(self.headers_mut(), |ext| {
            ext.query_params.push((key.into(), value.into()));
        });
        self
    }

    fn from_body<B: Into<Body>>(self, body: B) -> http::Result<Request<Body>> {
        self.body(body.into())
    }
}

#[async_trait]
pub trait RequestExt {
    /// Signature: `async fn send(self) -> Response<Body>`
    async fn send(self) -> Response<Body>;

    fn resolve_ureq_ext(self) -> Self;
}

#[async_trait]
impl RequestExt for Request<Body> {
    //
    async fn send(self) -> Response<Body> {
        //
        unimplemented!()
    }

    fn resolve_ureq_ext(self) -> Self {
        let (mut parts, body) = self.into_parts();
        resolve_ureq_ext(&mut parts);
        Request::from_parts(parts, body)
    }
}

static UREQ_CNT: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));
static UREQ_EXT: Lazy<Mutex<HashMap<usize, UreqExt>>> = Lazy::new(|| Mutex::new(HashMap::new()));

struct UreqExt {
    query_params: Vec<(String, String)>,
}

impl UreqExt {
    fn new() -> Self {
        UreqExt {
            query_params: vec![],
        }
    }
    fn invoke(self, parts: &mut http::request::Parts) {
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
    }
}

fn with_ureq_ext<F: FnOnce(&mut UreqExt)>(
    headers: Option<&mut http::HeaderMap<http::HeaderValue>>,
    f: F,
) {
    if let Some(headers) = headers {
        let val = headers
            .entry("x-ureq-ext")
            .or_insert_with(|| UREQ_CNT.fetch_add(1, Ordering::Relaxed).into());
        let id = val.to_str().unwrap().parse::<usize>().unwrap();
        let mut lock = UREQ_EXT.lock().unwrap();
        let ureq_ext = lock.entry(id).or_insert_with(UreqExt::new);
        f(ureq_ext);
    }
}

pub fn resolve_ureq_ext(parts: &mut http::request::Parts) {
    if let Some(val) = parts.headers.remove("x-ureq-ext") {
        let id = val.to_str().unwrap().parse::<usize>().unwrap();
        let mut lock = UREQ_EXT.lock().unwrap();
        if let Some(ureq_ext) = lock.remove(&id) {
            ureq_ext.invoke(parts);
        }
    }
}
