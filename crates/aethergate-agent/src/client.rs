//! HMAC-SHA256 signed HTTP client for `/api/open/v1/*` and SSE event streaming.

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use reqwest::StatusCode;
use reqwest::header::{ETAG, IF_NONE_MATCH};
use uuid::Uuid;

use aethergate_protocol::{
    CommandAckRequest, HEADER_NODE_ID, HEADER_NONCE, HEADER_SIGNATURE, HEADER_TIMESTAMP,
    NodeAuthVerifyResponse, NodeConfigResponse, NodeSseEvent, NodeStatusReport,
    NodeUsersSyncResponse, ReportAckResponse, TelemetryBatchRequest, sign_request,
};

/// Signed HTTP/2 client communicating with `aethergate-server`.
#[derive(Debug, Clone)]
pub struct OpenApiClient {
    /// Assigned node UUID.
    node_id: Uuid,
    /// Shared HMAC secret bytes.
    hmac_secret: Vec<u8>,
    /// Control plane base URL (without trailing slash).
    base_url: String,
    /// Reusable HTTP client.
    http: reqwest::Client,
    /// Cached `ETag` for `/api/open/v1/config`.
    last_config_etag: Option<String>,
}

impl OpenApiClient {
    /// Creates a new [`OpenApiClient`].
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be constructed.
    pub fn new(node_id: Uuid, hmac_secret: &str, base_url: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .context("failed to build reqwest client")?;

        Ok(Self {
            node_id,
            hmac_secret: hmac_secret.as_bytes().to_vec(),
            base_url: base_url.trim_end_matches('/').to_owned(),
            http,
            last_config_etag: None,
        })
    }

    /// Builds signed headers for a given method, path+query, and body.
    fn signed_headers(
        &self,
        method: &str,
        path_and_query: &str,
        body: &[u8],
    ) -> Result<reqwest::header::HeaderMap> {
        let ts = Utc::now().timestamp();
        let nonce = Uuid::new_v4().simple().to_string();
        let sig = sign_request(&self.hmac_secret, method, path_and_query, ts, &nonce, body)
            .map_err(|e| anyhow!("HMAC signing error: {e}"))?;

        let mut map = reqwest::header::HeaderMap::new();
        map.insert(HEADER_NODE_ID, self.node_id.to_string().parse()?);
        map.insert(HEADER_TIMESTAMP, ts.to_string().parse()?);
        map.insert(HEADER_NONCE, nonce.parse()?);
        map.insert(HEADER_SIGNATURE, sig.parse()?);
        Ok(map)
    }

    /// Calls `POST /api/open/v1/auth/verify`.
    ///
    /// # Errors
    /// Returns an error if the request or response decoding fails.
    pub async fn verify_auth(&self) -> Result<NodeAuthVerifyResponse> {
        let path = "/api/open/v1/auth/verify";
        let headers = self.signed_headers("POST", path, b"")?;
        let resp = self
            .http
            .post(format!("{}{path}", self.base_url))
            .headers(headers)
            .send()
            .await?
            .error_for_status()?
            .json::<NodeAuthVerifyResponse>()
            .await?;
        Ok(resp)
    }

    /// Calls `GET /api/open/v1/config?format=compact` with `If-None-Match` `ETag` caching.
    ///
    /// Returns `Ok(None)` on `304 Not Modified`.
    ///
    /// # Errors
    /// Returns an error if the HTTP request or JSON decoding fails.
    pub async fn fetch_compact_config(&mut self) -> Result<Option<NodeConfigResponse>> {
        let path = "/api/open/v1/config?format=compact";
        let mut headers = self.signed_headers("GET", path, b"")?;
        if let Some(etag) = &self.last_config_etag {
            headers.insert(IF_NONE_MATCH, format!("\"{etag}\"").parse()?);
        }

        let resp = self
            .http
            .get(format!("{}{path}", self.base_url))
            .headers(headers)
            .send()
            .await?;

        if resp.status() == StatusCode::NOT_MODIFIED {
            return Ok(None);
        }

        let resp = resp.error_for_status()?;
        if let Some(etag) = resp.headers().get(ETAG).and_then(|v| v.to_str().ok()) {
            self.last_config_etag = Some(etag.trim_matches('"').to_owned());
        }
        let cfg = resp.json::<NodeConfigResponse>().await?;
        Ok(Some(cfg))
    }

    /// Calls `GET /api/open/v1/users?since_rev=...`.
    ///
    /// # Errors
    /// Returns an error if the HTTP request or JSON decoding fails.
    pub async fn fetch_users(&self, since_rev: Option<i64>) -> Result<NodeUsersSyncResponse> {
        let path = match since_rev {
            Some(rev) => format!("/api/open/v1/users?since_rev={rev}"),
            None => "/api/open/v1/users".to_owned(),
        };
        let headers = self.signed_headers("GET", &path, b"")?;
        let resp = self
            .http
            .get(format!("{}{path}", self.base_url))
            .headers(headers)
            .send()
            .await?
            .error_for_status()?
            .json::<NodeUsersSyncResponse>()
            .await?;
        Ok(resp)
    }

    /// Calls `POST /api/open/v1/report/status`.
    ///
    /// # Errors
    /// Returns an error if the HTTP request fails.
    pub async fn report_status(
        &self,
        status: &NodeStatusReport,
    ) -> Result<Option<ReportAckResponse>> {
        let path = "/api/open/v1/report/status";
        let body = serde_json::to_vec(status)?;
        let headers = self.signed_headers("POST", path, &body)?;
        let resp = self
            .http
            .post(format!("{}{path}", self.base_url))
            .headers(headers)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await?
            .error_for_status()?;

        if resp.status() == StatusCode::NO_CONTENT {
            return Ok(None);
        }
        Ok(Some(resp.json::<ReportAckResponse>().await?))
    }

    /// Calls `POST /api/open/v1/report/telemetry`.
    ///
    /// # Errors
    /// Returns an error if the HTTP request fails.
    pub async fn report_telemetry(
        &self,
        batch: &TelemetryBatchRequest,
    ) -> Result<Option<ReportAckResponse>> {
        let path = "/api/open/v1/report/telemetry";
        let body = serde_json::to_vec(batch)?;
        let headers = self.signed_headers("POST", path, &body)?;
        let resp = self
            .http
            .post(format!("{}{path}", self.base_url))
            .headers(headers)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await?
            .error_for_status()?;

        if resp.status() == StatusCode::NO_CONTENT {
            return Ok(None);
        }
        Ok(Some(resp.json::<ReportAckResponse>().await?))
    }

    /// Calls `POST /api/open/v1/commands/{id}/ack`.
    ///
    /// # Errors
    /// Returns an error if the HTTP request fails.
    pub async fn ack_command(
        &self,
        command_id: Uuid,
        success: bool,
        message: Option<String>,
    ) -> Result<()> {
        let path = format!("/api/open/v1/commands/{command_id}/ack");
        let body = serde_json::to_vec(&CommandAckRequest { success, message })?;
        let headers = self.signed_headers("POST", &path, &body)?;
        self.http
            .post(format!("{}{path}", self.base_url))
            .headers(headers)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Opens `GET /api/open/v1/events` SSE stream and forwards parsed events to `tx`.
    ///
    /// # Errors
    /// Returns an error if the SSE stream fails to connect or drops.
    pub async fn listen_sse(&self, tx: tokio::sync::mpsc::Sender<NodeSseEvent>) -> Result<()> {
        let path = "/api/open/v1/events";
        let headers = self.signed_headers("GET", path, b"")?;
        let sse_client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .tcp_keepalive(std::time::Duration::from_secs(30))
            .build()
            .context("building SSE HTTP client")?;
        let mut resp = sse_client
            .get(format!("{}{path}", self.base_url))
            .headers(headers)
            .send()
            .await
            .context("connecting SSE stream")?
            .error_for_status()
            .context("SSE endpoint returned error status")?;

        let mut buf = String::new();
        while let Some(chunk) =
            tokio::time::timeout(std::time::Duration::from_secs(90), resp.chunk())
                .await
                .context("SSE keep-alive timed out after 90s")?
                .context("reading SSE chunk")?
        {
            let text = String::from_utf8_lossy(&chunk).replace("\r\n", "\n");
            buf.push_str(&text);
            while let Some(idx) = buf.find("\n\n") {
                let frame = buf[..idx].to_owned();
                buf.drain(..idx + 2);
                if let Some(ev) = parse_sse_frame(&frame)
                    && tx.send(ev).await.is_err()
                {
                    return Ok(());
                }
            }
        }
        Ok(())
    }
}

/// Parses a single SSE frame's `data:` lines into a [`NodeSseEvent`].
#[must_use]
pub fn parse_sse_frame(frame: &str) -> Option<NodeSseEvent> {
    let mut data_lines = Vec::new();
    for line in frame.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start());
        }
    }
    if data_lines.is_empty() {
        return None;
    }
    let joined = data_lines.join("\n");
    serde_json::from_str::<NodeSseEvent>(&joined).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sse_data_frame() {
        let frame = "event: config_changed\ndata: {\"event\":\"config_changed\",\"data\":{\"config_revision\":7}}";
        let parsed = parse_sse_frame(frame);
        assert_eq!(
            parsed,
            Some(NodeSseEvent::ConfigChanged { config_revision: 7 })
        );
    }
}
