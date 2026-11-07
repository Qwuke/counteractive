//! `manifest` rows: at most one accepted and one pending manifest.

use proto::{Blob, SignedManifest};
use rusqlite::OptionalExtension as _;

use super::{Journal, from_sql, to_sql};
use crate::WalletError;

/// Which of the two manifest rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestSlot {
    /// The one the wallet uses.
    Accepted,
    /// Fetched, waiting for the user's approval.
    Pending,
}

impl ManifestSlot {
    fn name(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Pending => "pending",
        }
    }
}

/// A stored manifest exactly as signed.
#[derive(Debug, Clone)]
pub struct StoredManifest {
    /// Its version, as read when it was stored.
    pub version: u64,
    /// The signed bytes and signature.
    pub signed: SignedManifest,
}

impl Journal {
    /// The manifest in `slot`, if any.
    pub fn manifest(&self, slot: ManifestSlot) -> Result<Option<StoredManifest>, WalletError> {
        let row = self
            .connection
            .query_row("SELECT version, body, signature FROM manifest WHERE status = ?1", [slot.name()], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?, row.get::<_, Vec<u8>>(2)?))
            })
            .optional()?;
        let Some((version, body, signature)) = row else { return Ok(None) };
        let signature = String::from_utf8(signature).map_err(|_| WalletError::Corrupt("manifest signature"))?;
        Ok(Some(StoredManifest {
            version: from_sql(version, "manifest version")?,
            signed: SignedManifest { manifest: Blob::new(body), signature },
        }))
    }

    /// Replaces the pending manifest.
    pub fn set_pending_manifest(&mut self, version: u64, signed: &SignedManifest) -> Result<(), WalletError> {
        self.write(|transaction| {
            transaction.execute(
                "INSERT OR REPLACE INTO manifest (status, version, body, signature) VALUES ('pending', ?1, ?2, ?3)",
                (to_sql(version)?, signed.manifest.as_bytes(), signed.signature.as_bytes()),
            )?;
            Ok(())
        })
    }

    /// Moves the pending manifest to accepted in one transaction, if its
    /// version is still `version` and not below the accepted one. The
    /// caller has run the accept rule on it.
    pub fn approve_pending_manifest(&mut self, version: u64) -> Result<(), WalletError> {
        self.write(|transaction| {
            let version = to_sql(version)?;
            let moved = transaction.execute(
                "INSERT OR REPLACE INTO manifest (status, version, body, signature)
                 SELECT 'accepted', version, body, signature FROM manifest
                 WHERE status = 'pending' AND version = ?1
                   AND version >= coalesce((SELECT version FROM manifest WHERE status = 'accepted'), 0)",
                [version],
            )?;
            if moved != 1 {
                return Err(WalletError::NoPendingManifest);
            }
            transaction.execute("DELETE FROM manifest WHERE status = 'pending'", [])?;
            Ok(())
        })
    }

    /// Drops the pending manifest, such as one equal to the accepted one.
    pub fn discard_pending_manifest(&mut self) -> Result<(), WalletError> {
        self.write(|transaction| {
            transaction.execute("DELETE FROM manifest WHERE status = 'pending'", [])?;
            Ok(())
        })
    }
}
