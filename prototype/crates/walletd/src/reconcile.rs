//! Settling what no conversation holds (docs 04 section 10): sessions
//! left behind by a crash, a lost connection or a failed close. Their
//! chains are in an unsettled state with no conversation attached.

use gate_conn::DialScope;
use proto::walletd::ReconcileReport;
use wallet_core::reconcile::settled_report;

use crate::error::ApiError;
use crate::state::{Context, Walletd};

impl Walletd {
    /// Drives every unattached unsettled chain to settled, each on its own
    /// connection, then reports every settled session. A chain that can't
    /// be settled now keeps its state and is reported as unresolved by the
    /// balance; the first such error is returned after trying the rest.
    ///
    /// The orphans are attached while the journal is held, so a second
    /// reconcile running at the same time leaves them to this one.
    pub async fn reconcile(&self, context: &Context) -> Result<ReconcileReport, ApiError> {
        let orphans: Vec<_> = {
            let journal = self.journal();
            journal
                .chains()?
                .into_iter()
                .filter(|row| row.is_unsettled() && !self.attached.contains(row.id))
                .map(|row| (row.id, self.attached.attach(row.id)))
                .collect()
        };
        let mut recovered = 0;
        let mut first_error = None;
        for (chain, _attachment) in orphans {
            let settled = async {
                let mut link = self.gate.connect(&context.manifest, DialScope::new()).await?;
                self.drive(context, &mut link, chain, true).await
            };
            match settled.await {
                Ok(_) => recovered += 1,
                Err(error) => {
                    tracing::warn!(chain = chain.0, code = error.code(), "could not settle a chain");
                    first_error.get_or_insert(error);
                }
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        let (sessions, frozen_chains) = settled_report(&self.journal())?;
        Ok(ReconcileReport { recovered, sessions, frozen_chains })
    }
}
