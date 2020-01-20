use crate::Error;
use std::fmt;

const DEFAULT_PORT_HTTP: &str = "80";
const DEFAULT_PORT_HTTPS: &str = "443";

pub enum HostPort<'a> {
    HasPort {
        host: &'a str,
        with_port: &'a str,
        is_tls: bool,
    },
    DefaultPort {
        host: &'a str,
        port: &'a str,
        is_tls: bool,
    },
}

impl<'a> HostPort<'a> {
    pub fn from_uri(uri: &'a http::Uri) -> Result<Self, Error> {
        let scheme = uri
            .scheme()
            .ok_or_else(|| format!("URI without scheme: {}", uri))?
            .as_str();

        let authority = uri
            .authority()
            .ok_or_else(|| format!("URI without authority: {}", uri))?;

        let has_port = authority.port().is_some();

        let hostport = if has_port {
            HostPort::HasPort {
                host: authority.host(),
                with_port: authority.as_str(),
                is_tls: scheme == "https",
            }
        } else {
            let scheme_default = match scheme {
                "http" => DEFAULT_PORT_HTTP,
                "https" => DEFAULT_PORT_HTTPS,
                _ => return Err(format!("Unknown URI scheme: {}", uri).into()),
            };
            HostPort::DefaultPort {
                host: authority.as_str(),
                port: scheme_default,
                is_tls: scheme == "https",
            }
        };

        Ok(hostport)
    }

    #[allow(unused)]
    pub fn host(&self) -> &str {
        match self {
            HostPort::HasPort { host, .. } => host,
            HostPort::DefaultPort { host, .. } => host,
        }
    }

    pub fn is_tls(&self) -> bool {
        match self {
            HostPort::HasPort { is_tls, .. } => *is_tls,
            HostPort::DefaultPort { is_tls, .. } => *is_tls,
        }
    }
}

impl<'a> fmt::Display for HostPort<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostPort::HasPort { with_port, .. } => write!(f, "{}", with_port),
            HostPort::DefaultPort { host, port, .. } => write!(f, "{}:{}", host, port),
        }
    }
}
