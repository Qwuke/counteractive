//! The storefront's endpoints over inner TLS (SPEC section 6).
//!
//! Handlers parse, call a payment, rule or gate function, and respond
//! (ARCHITECTURE 8.2 rule 3). Every failure is one [`ErrorCode`] from the
//! closed set, so no Stripe message, path or body reaches a client, and no
//! body is ever logged.

use std::path::PathBuf;
use std::sync::{Arc, PoisonError, RwLock};

use axum::extract::{DefaultBodyLimit, FromRequest, FromRequestParts, Path, Request, State};
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use manifest::ReleaseConfig;
use proto::gate::{AttestationRequest, LocalIssueRequest};
use proto::storefront::{
    CollectRequest, CollectResponse, ConfigureRequest, ConfigureResponse, CreateOrderRequest, CreateOrderResponse,
    StorefrontHealth, StorefrontState,
};
use proto::{Blob, ErrorBody, ErrorCode, SignedManifest};
use secrecy::ExposeSecret as _;
use serde::de::DeserializeOwned;
use serde_json::Value;
use zeroize::Zeroizing;

use crate::config;
use crate::gate::IssueClient;
use crate::order::{self, OrderId};
use crate::stripe::{PayError, PaymentProvider, StripeClient, StripeEndpoint};
use crate::tls::Attestor;

/// The largest request body accepted. The biggest request, collect, is a
/// 32-byte secret and an issue request of well under a kilobyte.
const MAX_REQUEST_BODY: usize = 64 * 1024;

/// The signed manifest's file names inside the manifest directory.
const MANIFEST_FILE: &str = "manifest.json";
const SIGNATURE_FILE: &str = "manifest.sig";

/// The longest Stripe key accepted. Stripe's restricted keys are about 100 characters.
const MAX_STRIPE_KEY: usize = 1024;

/// Everything the handlers share. No database: Stripe holds the orders and
/// the gate holds the issuance records.
pub struct AppState {
    release: ReleaseConfig,
    attestor: Attestor,
    manifest_dir: PathBuf,
    stripe: StripeEndpoint,
    /// `None` until the operator configures a Stripe key: state `unconfigured`.
    payments: RwLock<Option<Arc<dyn PaymentProvider>>>,
    gate: IssueClient,
}

impl AppState {
    /// Unconfigured state: no Stripe key yet.
    pub fn new(
        release: ReleaseConfig,
        attestor: Attestor,
        stripe: StripeEndpoint,
        manifest_dir: PathBuf,
        gate: IssueClient,
    ) -> Self {
        Self { release, attestor, manifest_dir, stripe, payments: RwLock::new(None), gate }
    }

    /// This boot's TLS identity and attestation source.
    pub fn attestor(&self) -> &Attestor {
        &self.attestor
    }

    fn state(&self) -> StorefrontState {
        if self.payments.read().unwrap_or_else(PoisonError::into_inner).is_some() {
            StorefrontState::Ready
        } else {
            StorefrontState::Unconfigured
        }
    }

    /// The payment provider, or `503 unconfigured` before the operator has sent a key.
    fn payments(&self) -> Result<Arc<dyn PaymentProvider>, ApiError> {
        let payments = self.payments.read().unwrap_or_else(PoisonError::into_inner);
        payments.clone().ok_or(ApiError(ErrorCode::Unconfigured))
    }
}

/// The storefront's routes, with JSON errors for unknown paths and oversized bodies.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/manifest", get(signed_manifest))
        .route("/v1/attestation", post(attestation))
        .route("/v1/orders", post(create_order))
        .route("/v1/orders/{id}/collect", post(collect))
        .route("/v1/operator/configure", post(configure))
        .fallback(|| async { ApiError(ErrorCode::NotFound) })
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY))
        .with_state(state)
}

/// A failed request: `{"error": code}` with the code's status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiError(pub ErrorCode);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.0.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(ErrorBody::new(self.0))).into_response()
    }
}

impl From<PayError> for ApiError {
    /// An unknown session is `404`; any other provider failure is `502`,
    /// logged without a body. Nothing is issued either way.
    fn from(error: PayError) -> Self {
        match error {
            PayError::NotFound => Self(ErrorCode::NotFound),
            other => {
                tracing::warn!(error = %other, "payment provider call failed");
                Self(ErrorCode::PaymentProvider)
            }
        }
    }
}

/// A JSON body whose rejections are API errors: `413 payload_too_large`
/// over the limit, `400 bad_request` for anything else malformed.
struct ApiJson<T>(T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for ApiJson<T> {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, ApiError> {
        match Json::<T>::from_request(request, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(rejection) if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE => {
                Err(ApiError(ErrorCode::PayloadTooLarge))
            }
            Err(_) => Err(ApiError(ErrorCode::BadRequest)),
        }
    }
}

/// Proof the request carried the storefront operator token as
/// `Authorization: Bearer <hex(token)>`.
///
/// As an extractor it runs before the body is read, so a caller without
/// the token learns nothing about what a valid body looks like.
struct Operator;

impl FromRequestParts<Arc<AppState>> for Operator {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Self, ApiError> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .and_then(|text| hex::decode(text.trim()).ok())
            .map(Zeroizing::new)
            .filter(|token| !token.is_empty());
        match token {
            Some(token) if config::is_operator_token(&state.release, &token) => Ok(Self),
            _ => Err(ApiError(ErrorCode::Unauthorized)),
        }
    }
}

/// `GET /v1/health`: `unconfigured` until a Stripe key is set, then `ready`.
async fn health(State(app): State<Arc<AppState>>) -> Json<StorefrontHealth> {
    Json(StorefrontHealth { state: app.state() })
}

/// `GET /v1/manifest`: the same signed manifest the gate serves, read from
/// disk on each request because the operator signs it only after the gate's
/// first unlock, while the storefront is already running.
///
/// The bytes are served in canonical form, which is what was signed; a
/// wallet checks the signature before trusting any of it.
async fn signed_manifest(State(app): State<Arc<AppState>>) -> Result<Json<SignedManifest>, ApiError> {
    let no_manifest = |error: std::io::Error| {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(%error, "reading the signed manifest failed");
        }
        ApiError(ErrorCode::NoManifest)
    };
    let bytes = tokio::fs::read(app.manifest_dir.join(MANIFEST_FILE)).await.map_err(no_manifest)?;
    let signature = tokio::fs::read_to_string(app.manifest_dir.join(SIGNATURE_FILE)).await.map_err(no_manifest)?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
        tracing::warn!("{MANIFEST_FILE} is not JSON");
        ApiError(ErrorCode::NoManifest)
    })?;
    Ok(Json(SignedManifest { manifest: Blob::new(manifest::canonical_json(&value)), signature }))
}

/// `POST /v1/attestation`: evidence binding this boot's TLS key, the
/// caller's nonce and role `storefront`.
async fn attestation(
    State(app): State<Arc<AppState>>,
    ApiJson(request): ApiJson<AttestationRequest>,
) -> Result<Json<attest::Evidence>, ApiError> {
    app.attestor.evidence(request.nonce).map(Json).map_err(|error| {
        tracing::error!(%error, "producing evidence failed");
        ApiError(ErrorCode::Internal)
    })
}

/// `POST /v1/orders`: opens a Checkout Session for a listed product,
/// tagged with the hash of the wallet's order secret.
async fn create_order(
    State(app): State<Arc<AppState>>,
    ApiJson(request): ApiJson<CreateOrderRequest>,
) -> Result<Json<CreateOrderResponse>, ApiError> {
    let payments = app.payments()?;
    let product = app.release.product(&request.product).ok_or(ApiError(ErrorCode::BadRequest))?;
    let checkout = payments.create_checkout(product, &hex::encode(request.secret_hash)).await?;
    Ok(Json(CreateOrderResponse { order_id: checkout.id.as_str().to_owned(), checkout_url: checkout.url }))
}

/// `POST /v1/orders/{id}/collect`: once Stripe says the order is paid, by
/// the caller, at the listed price, asks the gate to issue.
///
/// Nothing is kept between calls, so a retry repeats every check and the
/// gate returns its stored response for the same request.
async fn collect(
    State(app): State<Arc<AppState>>,
    Path(id): Path<String>,
    ApiJson(request): ApiJson<CollectRequest>,
) -> Result<Json<CollectResponse>, ApiError> {
    let payments = app.payments()?;
    let id = OrderId::parse(&id).ok_or(ApiError(ErrorCode::NotFound))?;
    let session = payments.get_checkout(&id).await?;
    let product = order::authorise(&app.release, &session, &request.order_secret).map_err(ApiError)?;
    let issue =
        LocalIssueRequest { order_ref: id.order_ref(), credits: product.credits, issue_request: request.issue_request };
    let issued = app.gate.issue(&issue).await.map_err(|error| {
        tracing::warn!(%error, "issuance through issue.sock failed");
        ApiError(error.wallet_code())
    })?;
    Ok(Json(CollectResponse { issue_response: issued.issue_response, credits: product.credits }))
}

/// `POST /v1/operator/configure`: takes the Stripe key, in memory only.
/// Configuring again replaces it.
async fn configure(
    State(app): State<Arc<AppState>>,
    _: Operator,
    ApiJson(request): ApiJson<ConfigureRequest>,
) -> Result<Json<ConfigureResponse>, ApiError> {
    let key = request.stripe_key.expose_secret();
    // The key becomes a header value, so it must be visible ASCII.
    if key.is_empty() || key.len() > MAX_STRIPE_KEY || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(ApiError(ErrorCode::BadRequest));
    }
    let client = StripeClient::new(app.stripe.clone(), request.stripe_key).map_err(|error| {
        tracing::error!(%error, "building the Stripe client failed");
        ApiError(ErrorCode::Internal)
    })?;
    *app.payments.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(client));
    tracing::info!("Stripe key configured: taking orders");
    Ok(Json(ConfigureResponse { state: StorefrontState::Ready }))
}
