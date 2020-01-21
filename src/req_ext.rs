//

pub trait RequestBuilderExt
where
    Self: Sized,
{
    fn query(self, key: &str, value: &str) -> http::Result<Self>;
}

impl RequestBuilderExt for http::request::Builder {
    //
    fn query(self, key: &str, value: &str) -> http::Result<Self> {
        let (parts, _) = self.body(()).map(|r| r.into_parts())?;
        let query = parts
            .uri
            .path_and_query()
            .map(|p| (p.path(), p.query().unwrap_or("")))
            .unwrap_or(("", ""));
        unimplemented!()
    }
}

pub trait RequestExt {}
