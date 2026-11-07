//! walletd's local API, over HTTP/1.1 on loopback. The address and bearer
//! token come from the files walletd writes in the wallet directory.

use std::net::SocketAddr;
use std::path::Path;

use anyhow::{Context as _, anyhow, bail};
use bytes::Bytes;
use http_body_util::{BodyExt as _, Full, Limited};
use hyper::body::Incoming;
use hyper::header::{AUTHORIZATION, CONTENT_TYPE, HOST};
use hyper::{Method, Request, Response};
use hyper_util::rt::TokioIo;
use proto::walletd::{CONVERSATION_HEADER, LocalErrorBody};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::net::TcpStream;
use zeroize::Zeroizing;

/// The file walletd writes its listening address to.
const ADDR_FILE: &str = "walletd.addr";
/// The file holding walletd's bearer token.
const TOKEN_FILE: &str = "walletd.token";
/// The largest JSON answer read.
const MAX_BODY: usize = 1024 * 1024;

/// A client for one walletd.
pub struct Walletd {
    addr: SocketAddr,
    token: SecretString,
}

impl Walletd {
    /// Finds the walletd serving this wallet directory.
    pub fn locate(wallet_dir: &Path) -> anyhow::Result<Self> {
        let addr_path = wallet_dir.join(ADDR_FILE);
        let addr = std::fs::read_to_string(&addr_path)
            .with_context(|| format!("reading {}: is walletd running?", addr_path.display()))?;
        let addr = addr.trim().parse().with_context(|| format!("{} is not an address", addr_path.display()))?;
        let token_path = wallet_dir.join(TOKEN_FILE);
        let token = Zeroizing::new(
            std::fs::read_to_string(&token_path).with_context(|| format!("reading {}", token_path.display()))?,
        );
        Ok(Self { addr, token: SecretString::from(token.trim()) })
    }

    /// `GET path`, decoded as JSON.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        let response = self.send(Method::GET, path, None, None).await?;
        read_json(response).await
    }

    /// `POST path` with a JSON body, decoded as JSON.
    pub async fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> anyhow::Result<T> {
        let response = self.send(Method::POST, path, Some(serde_json::to_vec(body)?), None).await?;
        read_json(response).await
    }

    /// `POST path` with a JSON body, returning the successful response for
    /// the caller to stream. `conversation` sets `X-Conversation-Id`.
    pub async fn post_streaming<B: Serialize>(
        &self,
        path: &str,
        body: &B,
        conversation: Option<&str>,
    ) -> anyhow::Result<Response<Incoming>> {
        let response = self.send(Method::POST, path, Some(serde_json::to_vec(body)?), conversation).await?;
        if !response.status().is_success() {
            return Err(error_from(response).await);
        }
        Ok(response)
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Vec<u8>>,
        conversation: Option<&str>,
    ) -> anyhow::Result<Response<Incoming>> {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(HOST, self.addr.to_string())
            .header(AUTHORIZATION, format!("Bearer {}", self.token.expose_secret()));
        if body.is_some() {
            request = request.header(CONTENT_TYPE, "application/json");
        }
        if let Some(id) = conversation {
            request = request.header(CONVERSATION_HEADER, id);
        }
        let request = request.body(Full::new(Bytes::from(body.unwrap_or_default())))?;

        let stream = TcpStream::connect(self.addr)
            .await
            .with_context(|| format!("connecting to walletd at {}: is it running?", self.addr))?;
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
        // The task ends with the connection, when the response body is done.
        tokio::spawn(connection);
        Ok(sender.send_request(request).await?)
    }
}

async fn read_json<T: DeserializeOwned>(response: Response<Incoming>) -> anyhow::Result<T> {
    if !response.status().is_success() {
        return Err(error_from(response).await);
    }
    let bytes = body_bytes(response).await?;
    serde_json::from_slice(&bytes).context("walletd sent an unexpected answer")
}

/// walletd's error body as an error: `code: message (needs N credits)`.
async fn error_from(response: Response<Incoming>) -> anyhow::Error {
    let status = response.status();
    let body = match body_bytes(response).await {
        Ok(bytes) => serde_json::from_slice::<LocalErrorBody>(&bytes).ok(),
        Err(error) => return error,
    };
    let Some(body) = body else { return anyhow!("walletd answered {status}") };
    let message = body.message.map(|message| format!(": {message}")).unwrap_or_default();
    let required =
        body.required.map(|required| format!(" (a session would need {required} credits)")).unwrap_or_default();
    anyhow!("{} {}{message}{required}", status.as_u16(), body.error)
}

async fn body_bytes(response: Response<Incoming>) -> anyhow::Result<Bytes> {
    match Limited::new(response.into_body(), MAX_BODY).collect().await {
        Ok(collected) => Ok(collected.to_bytes()),
        Err(error) => bail!("reading walletd's answer: {error}"),
    }
}
