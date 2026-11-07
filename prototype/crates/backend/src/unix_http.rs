//! One HTTP/1.1 exchange per connection over a Unix socket.
//!
//! A fresh connection per request keeps cancellation simple: the
//! connection belongs to the response, so dropping the response closes the
//! socket, and an OpenAI-compatible engine aborts a request whose client
//! has gone (docs 05 section 5, one debit rule).

use std::path::PathBuf;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt as _, Full, Limited};
use hyper::body::Incoming;
use hyper::header::{CONTENT_TYPE, HOST};
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::net::UnixStream;
use tokio::task::AbortHandle;

use crate::BackendError;

/// Posts JSON to an engine on a Unix socket.
#[derive(Debug, Clone)]
pub(crate) struct UnixHttp {
    socket: PathBuf,
    timeout: Duration,
}

/// A response whose connection lives exactly as long as it does.
pub(crate) struct Exchange {
    pub(crate) response: Response<Incoming>,
    pub(crate) connection: ConnectionGuard,
}

/// Aborts the connection task when dropped, closing the socket.
pub(crate) struct ConnectionGuard(AbortHandle);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl UnixHttp {
    /// A client for the engine at `socket`; `timeout` bounds connecting and
    /// waiting for response headers.
    pub(crate) fn new(socket: PathBuf, timeout: Duration) -> Self {
        Self { socket, timeout }
    }

    /// Sends `body` as JSON and returns the response once its headers arrive.
    /// A non-success status is an error.
    pub(crate) async fn post_json<T: Serialize>(&self, path: &str, body: &T) -> Result<Exchange, BackendError> {
        let json = serde_json::to_vec(body).map_err(|_| BackendError::Decode)?;
        let request = Request::post(path)
            .header(HOST, "localhost")
            .header(CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(json)))
            .map_err(|_| BackendError::Decode)?;
        let exchange =
            tokio::time::timeout(self.timeout, self.send(request)).await.map_err(|_| BackendError::Timeout)??;
        let status = exchange.response.status();
        if !status.is_success() {
            return Err(BackendError::Status(status.as_u16()));
        }
        Ok(exchange)
    }

    /// Posts JSON and decodes a JSON answer of at most `limit` bytes.
    pub(crate) async fn post_for_json<T: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
        limit: usize,
    ) -> Result<R, BackendError> {
        let Exchange { response, connection } = self.post_json(path, body).await?;
        let collect = Limited::new(response.into_body(), limit).collect();
        let bytes = tokio::time::timeout(self.timeout, collect)
            .await
            .map_err(|_| BackendError::Timeout)?
            .map_err(|error| match error.downcast::<hyper::Error>() {
                Ok(error) => BackendError::Http(*error),
                Err(_) => BackendError::BodyTooLarge,
            })?
            .to_bytes();
        drop(connection);
        serde_json::from_slice(&bytes).map_err(|_| BackendError::Decode)
    }

    async fn send(&self, request: Request<Full<Bytes>>) -> Result<Exchange, BackendError> {
        let stream = UnixStream::connect(&self.socket).await.map_err(BackendError::Connect)?;
        let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
        let task = tokio::spawn(async move {
            if let Err(error) = connection.await {
                tracing::debug!(%error, "engine connection ended with an error");
            }
        });
        let connection = ConnectionGuard(task.abort_handle());
        let response = sender.send_request(request).await?;
        Ok(Exchange { response, connection })
    }
}
