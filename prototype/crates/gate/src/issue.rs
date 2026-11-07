//! Issuance by `order_ref` on `issue.sock` (docs 05 section 5, Issuing;
//! ARCHITECTURE section 9).
//!
//! The same request always gets the same bytes, and a different request
//! for the same order gets `409 order_issued`. Nothing is purged, so a
//! lost response can be fetched again at any time.

use std::sync::Arc;

use crash_point::crash_point;
use manifest::ReleaseConfig;
use proto::Blob;
use proto::ErrorCode;
use proto::gate::{LocalIssueRequest, LocalIssueResponse};
use sha2::{Digest as _, Sha256};

use crate::blocking;
use crate::state::Unlocked;
use crate::store::IssueOutcome;

/// Answers a paid order's issue request. The caller has checked the gate is Ready.
pub async fn issue(
    unlocked: &Unlocked,
    release: &ReleaseConfig,
    request: LocalIssueRequest,
) -> Result<LocalIssueResponse, ErrorCode> {
    let LocalIssueRequest { order_ref, credits, issue_request } = request;
    let request_sha256: [u8; 32] = Sha256::digest(issue_request.as_bytes()).into();

    // 2: a stored answer for this order wins.
    match unlocked.store.issued(&order_ref) {
        Ok(Some(row)) if row.request_sha256 == request_sha256 => return Ok(answer(row.response)),
        Ok(Some(_)) => return Err(ErrorCode::OrderIssued),
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(%error, "reading an issuance row failed");
            return Err(ErrorCode::Retry);
        }
    }

    // 3: only a listed product's credits, so a storefront bug can't mint an arbitrary amount.
    if !release.has_product_with_credits(credits) {
        return Err(ErrorCode::BadRequest);
    }

    // 4 and 5: verify the request and answer it.
    let moderator = Arc::clone(&unlocked.moderator);
    let response = blocking(move || moderator.issue(credits, issue_request.as_bytes()))
        .await
        .map_err(|_| ErrorCode::BadRequest)?;

    // 6: record it, unless a row appeared meanwhile.
    let store = Arc::clone(&unlocked.store);
    let recorded = blocking(move || {
        let outcome = store.record_issue(&order_ref, &request_sha256, &response);
        outcome.map(|outcome| (outcome, response))
    })
    .await;
    match recorded {
        Ok((IssueOutcome::Recorded, response)) => {
            crash_point!("after_issue_commit");
            Ok(answer(response))
        }
        Ok((IssueOutcome::Existing { response }, _)) => Ok(answer(response)),
        Ok((IssueOutcome::Conflict, _)) => Err(ErrorCode::OrderIssued),
        Err(error) => {
            tracing::warn!(%error, "recording an issuance failed");
            Err(ErrorCode::Retry)
        }
    }
}

fn answer(response: Vec<u8>) -> LocalIssueResponse {
    LocalIssueResponse { issue_response: Blob::new(response) }
}
