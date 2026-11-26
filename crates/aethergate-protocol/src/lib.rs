//! Shared protocol contracts, 4-protocol `sing-box` 1.12+ compiler, HMAC auth,
//! and Hysteria 2-inspired Open API DTOs for Æthergate.

pub mod compiler;
pub mod hmac;
pub mod open_api;
pub mod protocol;
pub mod sse;
pub mod telemetry;

pub use compiler::{
    CompileError, LocalAgentOverrides, compile_singbox_config, compute_deterministic_etag,
    validate_inbound_ports,
};
pub use hmac::{
    DEFAULT_MAX_TIMESTAMP_SKEW_SECS, HEADER_NODE_ID, HEADER_NONCE, HEADER_SIGNATURE,
    HEADER_TIMESTAMP, HmacAuthError, VerifyRequestParams, build_canonical_string, sha256_hex,
    sign_request, verify_request,
};
pub use open_api::{
    ClientAuthRequest, ClientAuthResponse, CommandAckRequest, ConfigFormat, NodeAuthVerifyResponse,
    NodeConfigResponse, NodeUsersSyncResponse, UserDeltaEntry, UserDeltaOpKind, UserSyncMode,
};
pub use protocol::{
    AuthorizedUserCredential, Hy2ObfsConfig, InboundProtocolConfig, NodeInboundSpec, NodeTemplate,
    ProtocolType, TlsProfile, TransportLayer,
};
pub use sse::NodeSseEvent;
pub use telemetry::{
    InboundTelemetrySlice, NodeStatusReport, ReportAckAction, ReportAckResponse, ReportCadenceMode,
    TelemetryBatchRequest, TelemetrySample,
};

/// Returns the shared Open API / protocol contract version.
#[must_use]
pub const fn protocol_version() -> &'static str {
    "v1"
}
