//! `V2Ray` gRPC `StatsService` client and counter parser (`user>>>...`, `inbound>>>...`).

use std::collections::HashMap;

use anyhow::Result;
use tonic::transport::Channel;

/// Generated protobuf/gRPC types for `v2ray.core.app.stats.command`.
#[allow(
    missing_docs,
    clippy::missing_docs_in_private_items,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo
)]
pub mod proto {
    tonic::include_proto!("v2ray.core.app.stats.command");
}

use proto::stats_service_client::StatsServiceClient;
use proto::{QueryStatsRequest, Stat};

/// Parsed uplink/downlink byte counters from a single `QueryStats(reset=true)` call.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ParsedStatsSnapshot {
    /// Per-user `(uplink_bytes, downlink_bytes)` keyed by `proxy_username`.
    pub users: HashMap<String, (u64, u64)>,
    /// Per-inbound `(uplink_bytes, downlink_bytes)` keyed by `inbound_tag`.
    pub inbounds: HashMap<String, (u64, u64)>,
}

/// Parses `v2ray_api` `Stat` counters into user and inbound `(uplink, downlink)` maps.
#[must_use]
pub fn parse_stat_counters(stats: &[Stat]) -> ParsedStatsSnapshot {
    let mut snapshot = ParsedStatsSnapshot::default();

    for stat in stats {
        let value = u64::try_from(stat.value).unwrap_or(0);
        if value == 0 {
            continue;
        }
        let parts: Vec<&str> = stat.name.split(">>>").collect();
        if parts.len() != 4 || parts[2] != "traffic" {
            continue;
        }

        let entity_kind = parts[0];
        let entity_name = parts[1];
        let direction = parts[3];

        let target = match entity_kind {
            "user" => snapshot
                .users
                .entry(entity_name.to_owned())
                .or_insert((0, 0)),
            "inbound" => snapshot
                .inbounds
                .entry(entity_name.to_owned())
                .or_insert((0, 0)),
            _ => continue,
        };

        match direction {
            "uplink" => target.0 = target.0.saturating_add(value),
            "downlink" => target.1 = target.1.saturating_add(value),
            _ => {}
        }
    }

    snapshot
}

/// Thin wrapper around `StatsServiceClient` for querying and resetting counters.
#[derive(Debug, Clone)]
pub struct V2RayStatsClient {
    /// gRPC endpoint URI (e.g. `http://127.0.0.1:10085`).
    endpoint: String,
    /// Cached connected channel.
    channel: Option<Channel>,
}

impl V2RayStatsClient {
    /// Creates a new lazy gRPC stats client.
    #[must_use]
    pub fn new(addr: &str) -> Self {
        let endpoint = if addr.starts_with("http://") || addr.starts_with("https://") {
            addr.to_owned()
        } else {
            format!("http://{addr}")
        };
        Self {
            endpoint,
            channel: None,
        }
    }

    /// Queries all `user>>>` and `inbound>>>` traffic counters with `reset = true`.
    ///
    /// # Errors
    /// Returns an error if connecting or executing the gRPC call fails.
    pub async fn query_and_reset(&mut self) -> Result<ParsedStatsSnapshot> {
        if self.channel.is_none() {
            let chan = Channel::from_shared(self.endpoint.clone())?
                .connect_timeout(std::time::Duration::from_secs(2))
                .connect()
                .await?;
            self.channel = Some(chan);
        }

        let Some(chan) = self.channel.clone() else {
            return Ok(ParsedStatsSnapshot::default());
        };

        let mut client = StatsServiceClient::new(chan);
        #[allow(deprecated)]
        let req = QueryStatsRequest {
            pattern: String::new(),
            reset: true,
            patterns: vec!["user>>>".to_owned(), "inbound>>>".to_owned()],
            regexp: false,
        };

        match client.query_stats(req).await {
            Ok(resp) => Ok(parse_stat_counters(&resp.into_inner().stat)),
            Err(err) => {
                self.channel = None;
                Err(anyhow::anyhow!("v2ray stats gRPC error: {err}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_user_and_inbound_traffic_counters() {
        let raw = vec![
            Stat {
                name: "user>>>ag_alice>>>traffic>>>uplink".to_owned(),
                value: 1500,
            },
            Stat {
                name: "user>>>ag_alice>>>traffic>>>downlink".to_owned(),
                value: 8500,
            },
            Stat {
                name: "inbound>>>in-hy2-443>>>traffic>>>downlink".to_owned(),
                value: 8500,
            },
            Stat {
                name: "outbound>>>direct>>>traffic>>>uplink".to_owned(),
                value: 999,
            },
        ];

        let parsed = parse_stat_counters(&raw);
        assert_eq!(parsed.users.get("ag_alice"), Some(&(1500, 8500)));
        assert_eq!(parsed.inbounds.get("in-hy2-443"), Some(&(0, 8500)));
        assert!(!parsed.inbounds.contains_key("direct"));
    }
}
