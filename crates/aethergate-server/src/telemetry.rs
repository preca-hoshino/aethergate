//! Natural calendar month statistics and idempotent telemetry ingestion.
//!
//! All monthly traffic resets naturally on the 1st of each month (`stat_month = YYYY-MM-01`)
//! in `[stats].timezone`. No billing, multiplier, or quota logic exists here.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use aethergate_protocol::TelemetryBatchRequest;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Computes the 1st-of-month `DATE` for `timestamp` in the configured `stats_tz`.
#[must_use]
pub fn stat_month_for_timestamp(timestamp: DateTime<Utc>, tz: Tz) -> NaiveDate {
    let local = timestamp.with_timezone(&tz);
    NaiveDate::from_ymd_opt(local.year(), local.month(), 1)
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(2026, 1, 1).unwrap_or(NaiveDate::MIN))
}

/// Parses an optional `YYYY-MM` query string into the 1st-of-month `NaiveDate`.
/// Defaults to the current calendar month in `tz`.
///
/// # Errors
/// Returns [`AppError::BadRequest`] if the supplied month string is not valid `YYYY-MM`.
pub fn parse_stat_month(month_param: Option<&str>, tz: Tz) -> AppResult<NaiveDate> {
    let Some(raw) = month_param.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(stat_month_for_timestamp(Utc::now(), tz));
    };

    let full = format!("{raw}-01");
    NaiveDate::parse_from_str(&full, "%Y-%m-%d").map_err(|_| {
        AppError::BadRequest("month parameter must be formatted as YYYY-MM".to_owned())
    })
}

/// Computes `[start_utc, end_utc)` boundaries for a calendar month `stat_month` in `tz`.
#[must_use]
pub fn month_utc_bounds(stat_month: NaiveDate, tz: Tz) -> (DateTime<Utc>, DateTime<Utc>) {
    let (next_year, next_month) = if stat_month.month() == 12 {
        (stat_month.year() + 1, 1)
    } else {
        (stat_month.year(), stat_month.month() + 1)
    };

    let start_utc = stat_month
        .and_hms_opt(0, 0, 0)
        .and_then(|dt| tz.from_local_datetime(&dt).earliest())
        .map_or_else(Utc::now, |dt| dt.with_timezone(&Utc));

    let end_utc = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .and_then(|dt| tz.from_local_datetime(&dt).earliest())
        .map_or(start_utc, |dt| dt.with_timezone(&Utc));

    (start_utc, end_utc)
}

/// Ingests an idempotent telemetry batch inside a single PostgreSQL transaction.
///
/// Returns `Ok(true)` if newly ingested, `Ok(false)` if `batch_id` was already processed.
///
/// # Errors
/// Returns an error if database writes fail.
pub async fn ingest_telemetry_batch(
    state: &AppState,
    node_id: Uuid,
    batch: &TelemetryBatchRequest,
) -> AppResult<bool> {
    let mut tx: Transaction<'_, Postgres> = state.db.begin().await?;

    let inserted = sqlx::query(
        "INSERT INTO telemetry_batch_dedup (batch_id, node_id) VALUES ($1, $2) ON CONFLICT (batch_id) DO NOTHING",
    )
    .bind(batch.batch_id)
    .bind(node_id)
    .execute(&mut *tx)
    .await?;

    if inserted.rows_affected() == 0 {
        tx.commit().await?;
        return Ok(false);
    }

    let stat_month = stat_month_for_timestamp(batch.window_end, state.stats_tz);
    let (user_map, owner_map, has_child_logical_nodes) =
        load_telemetry_lookups(&mut tx, node_id, batch).await?;

    let mut node_uplink: i64 = 0;
    let mut node_downlink: i64 = 0;
    let mut node_peak_up: i64 = 0;
    let mut node_peak_down: i64 = 0;

    for sample in &batch.user_samples {
        let up = i64::try_from(sample.uplink_bytes).unwrap_or(i64::MAX);
        let down = i64::try_from(sample.downlink_bytes).unwrap_or(i64::MAX);
        let up_bps = i64::try_from(sample.uplink_bps).unwrap_or(i64::MAX);
        let down_bps = i64::try_from(sample.downlink_bps).unwrap_or(i64::MAX);

        node_uplink = node_uplink.saturating_add(up);
        node_downlink = node_downlink.saturating_add(down);
        node_peak_up = node_peak_up.max(up_bps);
        node_peak_down = node_peak_down.max(down_bps);

        if let Some((user_id, account_type)) = user_map.get(&sample.proxy_username)
            && account_type != "relay"
        {
            upsert_user_monthly_and_raw(
                &mut tx,
                stat_month,
                batch.window_end,
                node_id,
                *user_id,
                up,
                down,
                up_bps,
                down_bps,
            )
            .await?;
        }
    }

    let mut per_logical_node: HashMap<Uuid, (i64, i64)> = HashMap::new();
    for slice in &batch.inbound_slices {
        let target_node_id = owner_map
            .get(&slice.inbound_tag)
            .copied()
            .unwrap_or(node_id);
        let up = i64::try_from(slice.uplink_bytes).unwrap_or(i64::MAX);
        let down = i64::try_from(slice.downlink_bytes).unwrap_or(i64::MAX);
        let entry = per_logical_node.entry(target_node_id).or_insert((0, 0));
        entry.0 = entry.0.saturating_add(up);
        entry.1 = entry.1.saturating_add(down);

        upsert_inbound_monthly_and_raw(
            &mut tx,
            stat_month,
            batch.window_end,
            target_node_id,
            slice,
        )
        .await?;
    }

    if has_child_logical_nodes {
        for (logical_id, (up, down)) in per_logical_node {
            if up > 0 || down > 0 {
                upsert_node_monthly(&mut tx, stat_month, logical_id, up, down, 0, 0).await?;
            }
        }
    } else if node_uplink > 0 || node_downlink > 0 {
        upsert_node_monthly(
            &mut tx,
            stat_month,
            node_id,
            node_uplink,
            node_downlink,
            node_peak_up,
            node_peak_down,
        )
        .await?;
    }

    tx.commit().await?;
    state.mark_ws_dirty();
    Ok(true)
}

/// Resolved user and inbound-owner lookup maps for telemetry ingestion.
type TelemetryLookups = (HashMap<String, (Uuid, String)>, HashMap<String, Uuid>, bool);

/// Loads user account mappings and inbound-to-logical-node mappings inside the transaction.
async fn load_telemetry_lookups(
    tx: &mut Transaction<'_, Postgres>,
    node_id: Uuid,
    batch: &TelemetryBatchRequest,
) -> Result<TelemetryLookups, sqlx::Error> {
    let usernames: Vec<String> = batch
        .user_samples
        .iter()
        .map(|s| s.proxy_username.clone())
        .collect();

    let user_rows = if usernames.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as::<_, (Uuid, String, String)>(
            "SELECT id, proxy_username, account_type FROM users WHERE proxy_username = ANY($1)",
        )
        .bind(&usernames)
        .fetch_all(&mut **tx)
        .await?
    };
    let user_map: HashMap<String, (Uuid, String)> = user_rows
        .into_iter()
        .map(|(id, name, account_type)| (name, (id, account_type)))
        .collect();

    let inbound_owners = sqlx::query_as::<_, (String, Uuid)>(
        r"
        SELECT tag, node_id
        FROM node_inbounds
        WHERE node_id = $1
           OR node_id IN (SELECT id FROM nodes WHERE parent_node_id = $1)
        ",
    )
    .bind(node_id)
    .fetch_all(&mut **tx)
    .await?;
    let owner_map: HashMap<String, Uuid> = inbound_owners.into_iter().collect();
    let has_child_logical_nodes = owner_map.values().any(|&owner| owner != node_id);
    Ok((user_map, owner_map, has_child_logical_nodes))
}

/// Upserts `user_monthly_traffic`, `user_node_monthly_traffic`, and inserts a raw sample.
#[expect(clippy::too_many_arguments)]
async fn upsert_user_monthly_and_raw(
    tx: &mut Transaction<'_, Postgres>,
    stat_month: NaiveDate,
    window_end: DateTime<Utc>,
    node_id: Uuid,
    user_id: Uuid,
    up: i64,
    down: i64,
    up_bps: i64,
    down_bps: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        INSERT INTO user_monthly_traffic (
            stat_month, user_id, uplink_bytes, downlink_bytes, peak_uplink_bps, peak_downlink_bps, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, NOW())
        ON CONFLICT (stat_month, user_id) DO UPDATE SET
            uplink_bytes = user_monthly_traffic.uplink_bytes + EXCLUDED.uplink_bytes,
            downlink_bytes = user_monthly_traffic.downlink_bytes + EXCLUDED.downlink_bytes,
            peak_uplink_bps = GREATEST(user_monthly_traffic.peak_uplink_bps, EXCLUDED.peak_uplink_bps),
            peak_downlink_bps = GREATEST(user_monthly_traffic.peak_downlink_bps, EXCLUDED.peak_downlink_bps),
            updated_at = NOW()
        ",
    )
    .bind(stat_month)
    .bind(user_id)
    .bind(up)
    .bind(down)
    .bind(up_bps)
    .bind(down_bps)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r"
        INSERT INTO user_node_monthly_traffic (
            stat_month, user_id, node_id, uplink_bytes, downlink_bytes, peak_uplink_bps, peak_downlink_bps, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
        ON CONFLICT (stat_month, user_id, node_id) DO UPDATE SET
            uplink_bytes = user_node_monthly_traffic.uplink_bytes + EXCLUDED.uplink_bytes,
            downlink_bytes = user_node_monthly_traffic.downlink_bytes + EXCLUDED.downlink_bytes,
            peak_uplink_bps = GREATEST(user_node_monthly_traffic.peak_uplink_bps, EXCLUDED.peak_uplink_bps),
            peak_downlink_bps = GREATEST(user_node_monthly_traffic.peak_downlink_bps, EXCLUDED.peak_downlink_bps),
            updated_at = NOW()
        ",
    )
    .bind(stat_month)
    .bind(user_id)
    .bind(node_id)
    .bind(up)
    .bind(down)
    .bind(up_bps)
    .bind(down_bps)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r"
        INSERT INTO traffic_samples_raw (
            time, node_id, user_id, uplink_bytes, downlink_bytes, uplink_bps, downlink_bps
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ",
    )
    .bind(window_end)
    .bind(node_id)
    .bind(user_id)
    .bind(up)
    .bind(down)
    .bind(up_bps)
    .bind(down_bps)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Upserts `node_monthly_traffic` for the natural calendar month.
async fn upsert_node_monthly(
    tx: &mut Transaction<'_, Postgres>,
    stat_month: NaiveDate,
    node_id: Uuid,
    node_uplink: i64,
    node_downlink: i64,
    node_peak_up: i64,
    node_peak_down: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        INSERT INTO node_monthly_traffic (
            stat_month, node_id, uplink_bytes, downlink_bytes, peak_uplink_bps, peak_downlink_bps, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, NOW())
        ON CONFLICT (stat_month, node_id) DO UPDATE SET
            uplink_bytes = node_monthly_traffic.uplink_bytes + EXCLUDED.uplink_bytes,
            downlink_bytes = node_monthly_traffic.downlink_bytes + EXCLUDED.downlink_bytes,
            peak_uplink_bps = GREATEST(node_monthly_traffic.peak_uplink_bps, EXCLUDED.peak_uplink_bps),
            peak_downlink_bps = GREATEST(node_monthly_traffic.peak_downlink_bps, EXCLUDED.peak_downlink_bps),
            updated_at = NOW()
        ",
    )
    .bind(stat_month)
    .bind(node_id)
    .bind(node_uplink)
    .bind(node_downlink)
    .bind(node_peak_up)
    .bind(node_peak_down)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Upserts `node_inbound_monthly_traffic` and inserts a raw inbound sample.
async fn upsert_inbound_monthly_and_raw(
    tx: &mut Transaction<'_, Postgres>,
    stat_month: NaiveDate,
    window_end: DateTime<Utc>,
    node_id: Uuid,
    slice: &aethergate_protocol::InboundTelemetrySlice,
) -> Result<(), sqlx::Error> {
    let up = i64::try_from(slice.uplink_bytes).unwrap_or(i64::MAX);
    let down = i64::try_from(slice.downlink_bytes).unwrap_or(i64::MAX);
    let proto = serde_json::to_value(slice.protocol)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "vless_reality".to_owned());

    sqlx::query(
        r"
        INSERT INTO node_inbound_monthly_traffic (
            stat_month, node_id, inbound_tag, protocol, uplink_bytes, downlink_bytes, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, NOW())
        ON CONFLICT (stat_month, node_id, inbound_tag) DO UPDATE SET
            protocol = EXCLUDED.protocol,
            uplink_bytes = node_inbound_monthly_traffic.uplink_bytes + EXCLUDED.uplink_bytes,
            downlink_bytes = node_inbound_monthly_traffic.downlink_bytes + EXCLUDED.downlink_bytes,
            updated_at = NOW()
        ",
    )
    .bind(stat_month)
    .bind(node_id)
    .bind(&slice.inbound_tag)
    .bind(&proto)
    .bind(up)
    .bind(down)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r"
        INSERT INTO node_inbound_samples_raw (
            time, node_id, inbound_tag, protocol, uplink_bytes, downlink_bytes
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        ",
    )
    .bind(window_end)
    .bind(node_id)
    .bind(&slice.inbound_tag)
    .bind(&proto)
    .bind(up)
    .bind(down)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_month_respects_timezone_boundary() {
        // 2026-11-30 16:30:00 UTC is 2026-12-01 00:30:00 Asia/Shanghai
        let ts = Utc.with_ymd_and_hms(2026, 11, 30, 16, 30, 0).single();
        assert!(ts.is_some());
        if let Some(dt) = ts {
            let shanghai = stat_month_for_timestamp(dt, chrono_tz::Asia::Shanghai);
            let utc = stat_month_for_timestamp(dt, chrono_tz::UTC);
            assert_eq!(shanghai.to_string(), "2026-12-01");
            assert_eq!(utc.to_string(), "2026-11-01");
        }
    }

    #[test]
    fn parse_stat_month_validates_format() {
        let ok = parse_stat_month(Some("2026-11"), chrono_tz::UTC);
        assert!(ok.is_ok());
        let bad = parse_stat_month(Some("2026/11"), chrono_tz::UTC);
        assert!(bad.is_err());
    }

    #[test]
    fn month_utc_bounds_handles_timezone_and_year_rollover() {
        let Some(nov) = NaiveDate::from_ymd_opt(2026, 11, 1) else {
            return;
        };
        let (start, end) = month_utc_bounds(nov, chrono_tz::Asia::Shanghai);
        assert_eq!(start.to_rfc3339(), "2026-10-31T16:00:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-11-30T16:00:00+00:00");

        let Some(dec) = NaiveDate::from_ymd_opt(2026, 12, 1) else {
            return;
        };
        let (dec_start, dec_end) = month_utc_bounds(dec, chrono_tz::UTC);
        assert_eq!(dec_start.to_rfc3339(), "2026-12-01T00:00:00+00:00");
        assert_eq!(dec_end.to_rfc3339(), "2027-01-01T00:00:00+00:00");
    }
}
