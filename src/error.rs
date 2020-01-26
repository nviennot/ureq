use crate::h1;
use std::fmt;
use std::io;
use std::string::FromUtf8Error;
use tls_api::Error as TlsError;

#[derive(Debug)]
pub enum Error {
    Message(String),
    Static(&'static str),
    Io(io::Error),
    TlsError(tls_api::Error),
    H1(h1::Error),
    H2(h2::Error),
    Http(http::Error),
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

impl From<TlsError> for Error {
    fn from(e: TlsError) -> Self {
        Error::TlsError(e)
    }
}

impl From<h1::Error> for Error {
    fn from(e: h1::Error) -> Self {
        Error::H1(e)
    }
}

impl From<h2::Error> for Error {
    fn from(e: h2::Error) -> Self {
        Error::H2(e)
    }
}

impl From<http::Error> for Error {
    fn from(e: http::Error) -> Self {
        Error::Http(e)
    }
}

impl From<FromUtf8Error> for Error {
    fn from(e: FromUtf8Error) -> Self {
        Error::FromUtf8(e)
    }
}
