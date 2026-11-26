//! Natural calendar month statistics and idempotent telemetry ingestion.
//!
//! All monthly traffic resets naturally on the 1st of each month (`stat_month = YYYY-MM-01`)
//! in `[stats].timezone`. No billing, multiplier, or quota logic exists here.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, NaiveDate, Utc};
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
    let usernames: Vec<String> = batch
        .user_samples
        .iter()
        .map(|s| s.proxy_username.clone())
        .collect();

    let user_rows = if usernames.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as::<_, (Uuid, String)>(
            "SELECT id, proxy_username FROM users WHERE proxy_username = ANY($1)",
        )
        .bind(&usernames)
        .fetch_all(&mut *tx)
        .await?
    };
    let user_map: HashMap<String, Uuid> =
        user_rows.into_iter().map(|(id, name)| (name, id)).collect();

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

        if let Some(&user_id) = user_map.get(&sample.proxy_username) {
            upsert_user_monthly_and_raw(
                &mut tx,
                stat_month,
                batch.window_end,
                node_id,
                user_id,
                up,
                down,
                up_bps,
                down_bps,
            )
            .await?;
        }
    }

    if node_uplink > 0 || node_downlink > 0 {
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

    for slice in &batch.inbound_slices {
        upsert_inbound_monthly_and_raw(&mut tx, stat_month, batch.window_end, node_id, slice)
            .await?;
    }

    tx.commit().await?;
    state.mark_ws_dirty();
    Ok(true)
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
    use chrono::TimeZone;

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
}
