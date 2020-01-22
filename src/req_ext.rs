use crate::Body;
use async_trait::async_trait;
use http::request;
use http::Uri;
use http::{Request, Response};
use qstring::QString;

pub trait RequestBuilderExt
where
    Self: Sized,
{
    fn query(self, key: &str, value: &str) -> http::Result<Self>;
}

impl RequestBuilderExt for request::Builder {
    //
    fn query(self, key: &str, value: &str) -> http::Result<Self> {
        // Parts is private in Builder. We have to body() to get it out.
        let req = self.body(())?;
        let (req_parts, _) = req.into_parts();
        let mut uri_parts = req_parts.uri.into_parts();

        // Construct new instance of PathAndQuery with our modified query.
        let new_path_and_query = {
            //
            let (path, query) = uri_parts
                .path_and_query
                .as_ref()
                .map(|p| (p.path(), p.query().unwrap_or("")))
                .unwrap_or(("", ""));

            let mut qs = QString::from(query);
            qs.add_pair((key, value));

            // PathAndQuery has no API for modifying any fields. This seems to be our only
            // option to get a new instance of it using the public API.
            let tmp: Uri = format!("http://fake{}?{}", path, qs).parse().unwrap();
            let tmp_parts = tmp.into_parts();
            tmp_parts.path_and_query.unwrap()
        };

        // This is good. We can change the PathAndQuery field.
        uri_parts.path_and_query = Some(new_path_and_query);
        let new_uri = Uri::from_parts(uri_parts).unwrap();

        // The result of this RequestBuilderExt is a Builder, but we don't have any
        // RequestBuilder::from_parts(), which means we're forced to start over.
        let mut builder = Request::builder()
            .method(req_parts.method)
            .uri(new_uri)
            .version(req_parts.version);

        // A Builder.headers() taking a complete HeaderMap would help here.
        for (name, value) in req_parts.headers.iter() {
            builder = builder.header(name, value);
        }

        // The Extensions type doesn't seem to be possible to query and/or move over
        // to the new builder. This needs some solution.
        // req_parts.extensions

        Ok(builder)
    }
}

#[async_trait]
pub trait RequestExt {
    /// Signature: `async fn send(self) -> Response<Body>`
    async fn send(self) -> Response<Body>;
}

#[async_trait]
impl RequestExt for Request<Body> {
    //
    async fn send(self) -> Response<Body> {
        //
        unimplemented!()
    }
}
