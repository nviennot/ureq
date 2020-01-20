use std::mem;

// This struct must exactly match that of http::request::Builder,
// but since the original isn't repr C, we can't really guarantee
// it works...
#[derive(Debug)]
struct ReqBuilder {
    inner: http::Result<http::request::Parts>,
}

fn into_local_type(b: http::request::Builder) -> ReqBuilder {
    unsafe { mem::transmute(b) }
}

pub trait RequestBuilderExt {
    fn query(self, key: &str, value: &str) -> Self;
}

impl RequestBuilderExt for http::request::Builder {
    //
    fn query(self, key: &str, value: &str) -> Self {
        let inner = into_local_type(self).inner.map(|mut parts| {
            //
            let uri = parts.uri;
            let mut ubld = http::uri::Uri::builder();
            // TODO investigate if we can get rid of these clones by talking to upstream.
            // Something like Uri.into_parts().
            if let Some(scheme) = uri.scheme() {
                ubld = ubld.scheme(scheme.clone());
            }
            if let Some(authority) = uri.authority() {
                ubld = ubld.authority(authority.clone());
            }
            if let Some(path_and_query) = uri.path_and_query() {
                let path = path_and_query.path();
                let query = path_and_query.query().unwrap_or("");
                ubld = ubld.path_and_query("");
            }
            parts.uri = ubld.build().expect("Failed to build new URI");
            parts
        });

        unimplemented!()
    }
}

pub trait RequestExt {}
