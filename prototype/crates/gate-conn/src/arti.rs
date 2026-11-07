//! Dials onion services through an in-process Arti client: `walletd` only.

use std::path::Path;
use std::sync::Arc;

use arti_client::config::{BoolOrAuto, TorClientConfigBuilder};
use arti_client::{StreamPrefs, TorClient};
use async_trait::async_trait;
use tor_rtcompat::PreferredRuntime;

use crate::Target;
use crate::dialer::{BoxedIo, DialError, DialScope, Dialer};

/// Opens streams to onion services. Each [`DialScope`] gets its own circuits.
#[derive(Clone)]
pub struct ArtiDialer {
    client: Arc<TorClient<PreferredRuntime>>,
}

impl ArtiDialer {
    /// Bootstraps a Tor client keeping its state and cache in these directories.
    pub async fn bootstrap(state_dir: &Path, cache_dir: &Path) -> Result<Self, DialError> {
        let config = TorClientConfigBuilder::from_directories(state_dir, cache_dir)
            .build()
            .map_err(|error| DialError::Io(std::io::Error::other(error)))?;
        Ok(Self { client: TorClient::create_bootstrapped(config).await? })
    }

    /// Wraps a client the caller already bootstrapped.
    pub fn from_client(client: Arc<TorClient<PreferredRuntime>>) -> Self {
        Self { client }
    }
}

impl std::fmt::Debug for ArtiDialer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ArtiDialer")
    }
}

#[async_trait]
impl Dialer for ArtiDialer {
    async fn dial(&self, target: &Target, scope: &DialScope) -> Result<BoxedIo, DialError> {
        let Target::Onion { host, port } = target else {
            return Err(DialError::UnsupportedTarget(target.kind()));
        };
        let mut prefs = StreamPrefs::new();
        prefs.connect_to_onion_services(BoolOrAuto::Explicit(true));
        prefs.set_isolation(scope.isolation);
        let stream = self.client.connect_with_prefs((host.as_str(), *port), &prefs).await?;
        Ok(Box::new(stream))
    }
}
