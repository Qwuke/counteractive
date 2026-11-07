//! The storefront's only route to the gate: `POST /local/issue` on
//! `issue.sock`, plain HTTP/1.1 over a local Unix socket (SPEC section 5).

use std::path::PathBuf;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt as _, Full, Limited};
use hyper::header::{CONTENT_TYPE, HOST};
use hyper::{Method, Request, StatusCode};
use hyper_util::rt::TokioIo;
use proto::gate::{LocalIssueRequest, LocalIssueResponse};
use proto::{ErrorBody, ErrorCode};
use tokio::net::UnixStream;

/// A whole issuance call must finish within this. Issuing is one proof
/// check and one signature, so only a stuck gate takes longer.
const ISSUE_TIMEOUT: Duration = Duration::from_secs(30);

/// The largest response read from the gate. An issue response is a few
/// hundred bytes.
const MAX_RESPONSE: usize = 64 * 1024;

/// Why the gate didn't issue.
#[derive(Debug, thiserror::Error)]
pub enum GateError {
    /// Nothing is listening on `issue.sock`: the gate is down or restarting.
    #[error("issue.sock unreachable: {0}")]
    Unreachable(std::io::Error),
    /// The HTTP exchange failed part way.
    #[error("issue.sock HTTP failed: {0}")]
    Http(#[from] hyper::Error),
    /// The gate took longer than the issuance timeout.
    #[error("issue.sock timed out")]
    Timeout,
    /// The gate answered with an error status.
    #[error("gate refused issuance with status {status}")]
    Refused {
        /// The HTTP status.
        status: u16,
        /// The gate's error code, when its body was one.
        code: Option<ErrorCode>,
    },
    /// The gate's answer wasn't a readable issue response.
    #[error("gate sent an unusable response")]
    Malformed,
}

impl GateError {
    /// What the wallet is told.
    ///
    /// The gate's `400`, `409 order_issued` and `503` answers pass through,
    /// since the wallet's reaction depends on them (docs 04 section 8). A
    /// gate that can't be reached is reported as `503 locked`, which tells
    /// the wallet to retry later: a paid order stays collectable.
    pub fn wallet_code(&self) -> ErrorCode {
        use ErrorCode::{BadRequest, Internal, Locked, LowDisk, OrderIssued, Retry};
        match self {
            Self::Refused { code: Some(code @ (BadRequest | OrderIssued | Locked | LowDisk | Retry)), .. } => *code,
            Self::Refused { status: 400, .. } => BadRequest,
            Self::Refused { status: 503, .. } | Self::Unreachable(_) | Self::Http(_) | Self::Timeout => Locked,
            Self::Refused { .. } | Self::Malformed => Internal,
        }
    }
}

/// Calls the gate's `issue.sock`, one connection per call. Issuance is rare
/// and a fresh connection never carries a half-read response from the last.
#[derive(Debug, Clone)]
pub struct IssueClient {
    socket: PathBuf,
}

impl IssueClient {
    /// A client for the socket at `socket`. Nothing connects until [`IssueClient::issue`].
    pub fn new(socket: PathBuf) -> Self {
        Self { socket }
    }

    /// Asks the gate to issue. A repeated request for the same order gets
    /// the gate's stored response.
    pub async fn issue(&self, request: &LocalIssueRequest) -> Result<LocalIssueResponse, GateError> {
        let body = serde_json::to_vec(request).expect("an issue request always serialises");
        let (status, body) =
            tokio::time::timeout(ISSUE_TIMEOUT, self.post(body)).await.map_err(|_| GateError::Timeout)??;
        if status.is_success() {
            return serde_json::from_slice(&body).map_err(|_| GateError::Malformed);
        }
        let code = serde_json::from_slice::<ErrorBody>(&body).ok().map(|error| error.error);
        Err(GateError::Refused { status: status.as_u16(), code })
    }

    async fn post(&self, body: Vec<u8>) -> Result<(StatusCode, Bytes), GateError> {
        let stream = UnixStream::connect(&self.socket).await.map_err(GateError::Unreachable)?;
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
        tokio::spawn(async move {
            if let Err(error) = connection.await {
                tracing::debug!(%error, "issue.sock connection ended with an error");
            }
        });
        let request = Request::builder()
            .method(Method::POST)
            .uri("/local/issue")
            .header(HOST, "localhost")
            .header(CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(body)))
            .expect("a fixed path and headers always build");
        let response = sender.send_request(request).await?;
        let status = response.status();
        let body = match Limited::new(response.into_body(), MAX_RESPONSE).collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(error) => {
                return Err(match error.downcast::<hyper::Error>() {
                    Ok(error) => GateError::Http(*error),
                    Err(_) => GateError::Malformed,
                });
            }
        };
        Ok((status, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_answers_map_to_what_the_wallet_reacts_to() {
        let refused = |status, code| GateError::Refused { status, code };
        let cases = [
            (refused(409, Some(ErrorCode::OrderIssued)), ErrorCode::OrderIssued),
            (refused(503, Some(ErrorCode::Locked)), ErrorCode::Locked),
            (refused(503, Some(ErrorCode::LowDisk)), ErrorCode::LowDisk),
            (refused(400, Some(ErrorCode::BadRequest)), ErrorCode::BadRequest),
            (refused(400, None), ErrorCode::BadRequest),
            (refused(503, Some(ErrorCode::Unknown)), ErrorCode::Locked),
            (refused(500, None), ErrorCode::Internal),
            // A code the issue endpoint never sends doesn't pass through.
            (refused(409, Some(ErrorCode::Spent)), ErrorCode::Internal),
            (GateError::Timeout, ErrorCode::Locked),
            (GateError::Unreachable(std::io::ErrorKind::NotFound.into()), ErrorCode::Locked),
            (GateError::Malformed, ErrorCode::Internal),
        ];
        for (error, code) in cases {
            assert_eq!(error.wallet_code(), code, "{error:?}");
        }
    }
}
