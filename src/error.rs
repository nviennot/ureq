use std::fmt;
use std::io;
use std::string::FromUtf8Error;

#[cfg(feature = "tls")]
use tls_api::Error as TlsError;

#[derive(Debug)]
pub enum Error {
    Message(String),
    Static(&'static str),
    Io(io::Error),
    #[cfg(feature = "tls")]
    TlsError(tls_api::Error),
    H2(h2::Error),
    Http11Parser(httparse::Error),
    HttpApi(http::Error),
    FromUtf8(FromUtf8Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for Error {}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Message(s)
    }
}

impl<'a> From<&'a str> for Error {
    fn from(s: &'a str) -> Self {
        Error::Message(s.to_owned())
    }
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

#[cfg(feature = "tls")]
impl From<TlsError> for Error {
    fn from(e: TlsError) -> Self {
        Error::TlsError(e)
    }
}

impl From<h2::Error> for Error {
    fn from(e: h2::Error) -> Self {
        Error::H2(e)
    }
}

impl From<httparse::Error> for Error {
    fn from(e: httparse::Error) -> Self {
        Error::Http11Parser(e)
    }
}

impl From<http::Error> for Error {
    fn from(e: http::Error) -> Self {
        Error::HttpApi(e)
    }
}

impl From<FromUtf8Error> for Error {
    fn from(e: FromUtf8Error) -> Self {
        Error::FromUtf8(e)
    }
}
