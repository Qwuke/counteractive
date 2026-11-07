//! `/wallet/*`: balance, close, reconcile and the manifest.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use proto::walletd::{
    BalanceView, ManifestAction, ManifestActionRequest, ManifestStatus, ReconcileReport, WalletCloseRequest,
    WalletCloseResponse,
};
use wallet_core::manifest_store::{self, Offered};
use wallet_core::reconcile::balance_view;

use super::{ApiJson, to_completion};
use crate::error::ApiError;
use crate::session::Reached;
use crate::state::Walletd;

/// `GET /wallet/balance` (docs 04 section 7).
pub async fn balance(State(walletd): State<Arc<Walletd>>) -> Result<Json<BalanceView>, ApiError> {
    let context = walletd.context()?;
    let view = balance_view(&walletd.journal(), &context.holder, &walletd.pins, &context.manifest)?;
    Ok(Json(view))
}

/// `POST /wallet/close`: settles one conversation's session, or every one.
pub async fn close(
    State(walletd): State<Arc<Walletd>>,
    ApiJson(request): ApiJson<WalletCloseRequest>,
) -> Result<Json<WalletCloseResponse>, ApiError> {
    let context = walletd.context()?;
    let conversations = match request.conversation_id {
        Some(id) => walletd.conversations.get(&id).map(|conversation| vec![(id, conversation)]).unwrap_or_default(),
        None => walletd.conversations.all(),
    };
    to_completion(async move {
        let mut response = WalletCloseResponse { closed: 0, recovered: 0 };
        for (id, conversation) in conversations {
            let mut conversation = conversation.lock().await;
            if let Some(Reached::Settled { via_recover, .. }) =
                walletd.close_conversation(&context, &mut conversation).await?
            {
                response.closed += 1;
                response.recovered += u64::from(via_recover);
            }
            walletd.conversations.remove(&id);
        }
        Ok(Json(response))
    })
    .await
}

/// `POST /wallet/reconcile`: settles what no conversation holds, then reports.
pub async fn reconcile(State(walletd): State<Arc<Walletd>>) -> Result<Json<ReconcileReport>, ApiError> {
    let context = walletd.context()?;
    to_completion(async move { Ok(Json(walletd.reconcile(&context).await?)) }).await
}

/// `GET /wallet/manifest`.
pub async fn manifest_status(State(walletd): State<Arc<Walletd>>) -> Result<Json<ManifestStatus>, ApiError> {
    Ok(Json(manifest_store::status(&walletd.journal(), walletd.accept_pins())?))
}

/// `POST /wallet/manifest`: fetch the gate's manifest into pending, or
/// approve the pending one. Fetching uses the gate's dialer, so at use
/// time a manifest only ever comes over the use plane (docs 04 section 6).
pub async fn manifest_action(
    State(walletd): State<Arc<Walletd>>,
    ApiJson(request): ApiJson<ManifestActionRequest>,
) -> Result<Json<ManifestStatus>, ApiError> {
    match request.action {
        ManifestAction::Fetch => {
            let signed = walletd.gate.fetch_manifest().await?;
            let offered = manifest_store::offer(&mut walletd.journal(), walletd.accept_pins(), &signed)
                .map_err(|error| ApiError::local(StatusCode::BAD_GATEWAY, "manifest_rejected", error.to_string()))?;
            if let Offered::Pending(manifest) = offered {
                tracing::info!(version = manifest.version, "a fetched manifest is waiting for approval");
            }
        }
        ManifestAction::Approve => {
            manifest_store::approve(&mut walletd.journal(), walletd.accept_pins())?;
        }
    }
    Ok(Json(manifest_store::status(&walletd.journal(), walletd.accept_pins())?))
}
