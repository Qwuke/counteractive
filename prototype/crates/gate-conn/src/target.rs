//! Where a connection goes: `unix:/path`, `tcp:host:port` or `onion:host.onion:443`.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A dial target. Which dialer can reach it depends on the kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Target {
    /// A Unix socket: the gate's `gate.sock` in tier 1 tests.
    Unix(PathBuf),
    /// A TCP address: the storefront, from `wallet-cli` and `xtask`.
    Tcp {
        /// Host name or IP address, without brackets.
        host: String,
        /// Port.
        port: u16,
    },
    /// An onion service: the gate, from `walletd` over Arti.
    Onion {
        /// The `.onion` host.
        host: String,
        /// Port.
        port: u16,
    },
}

impl Target {
    /// The kind's prefix, for messages.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Unix(_) => "unix",
            Self::Tcp { .. } => "tcp",
            Self::Onion { .. } => "onion",
        }
    }
}

/// A target string that didn't parse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid target `{0}`: expected unix:/path, tcp:host:port or onion:host.onion:port")]
pub struct TargetError(String);

impl FromStr for Target {
    type Err = TargetError;

    #[expect(
        clippy::case_sensitive_file_extension_comparisons,
        reason = "`.onion` is a hostname suffix here, not a file extension"
    )]
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let error = || TargetError(text.to_owned());
        let (kind, rest) = text.split_once(':').ok_or_else(error)?;
        match kind {
            "unix" if !rest.is_empty() => Ok(Self::Unix(PathBuf::from(rest))),
            "tcp" => {
                let (host, port) = host_port(rest).ok_or_else(error)?;
                Ok(Self::Tcp { host, port })
            }
            "onion" => {
                let (host, port) = host_port(rest).ok_or_else(error)?;
                if !host.ends_with(".onion") {
                    return Err(error());
                }
                Ok(Self::Onion { host, port })
            }
            _ => Err(error()),
        }
    }
}

/// Splits `host:port`, accepting `[v6]:port`.
fn host_port(text: &str) -> Option<(String, u16)> {
    let (host, port) = text.rsplit_once(':')?;
    let host = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    if host.is_empty() {
        return None;
    }
    Some((host.to_owned(), port.parse().ok()?))
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unix(path) => write!(f, "unix:{}", path.display()),
            Self::Tcp { host, port } if host.contains(':') => write!(f, "tcp:[{host}]:{port}"),
            Self::Tcp { host, port } => write!(f, "tcp:{host}:{port}"),
            Self::Onion { host, port } => write!(f, "onion:{host}:{port}"),
        }
    }
}

impl Serialize for Target {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Target {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints_every_kind() {
        for text in ["unix:/run/gate/gate.sock", "tcp:127.0.0.1:8443", "tcp:[::1]:8443", "onion:abc.onion:443"] {
            let target: Target = text.parse().unwrap();
            assert_eq!(target.to_string(), text);
        }
        assert_eq!("tcp:[::1]:8443".parse::<Target>().unwrap(), Target::Tcp { host: "::1".into(), port: 8443 });
    }

    #[test]
    fn rejects_malformed_targets() {
        for text in ["", "unix:", "tcp:host", "tcp::80", "tcp:h:99999", "onion:example.com:443", "http://x"] {
            assert!(text.parse::<Target>().is_err(), "{text}");
        }
    }
}
