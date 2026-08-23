//! GTFS-Realtime: live arrival predictions.
//!
//! Two things about OC Transpo's feed shape the code here:
//!
//!  * The JSON is a .NET serialisation — PascalCase field names with a `HasX`
//!    boolean beside every optional `X`. It is *not* the standard GTFS-RT JSON
//!    mapping, so off-the-shelf crates won't parse it.
//!  * There is no `Delay` field. Predictions arrive as absolute epoch seconds
//!    in `Arrival.Time`, so lateness is ours to compute against the schedule.
//!
//! Coverage measured against the live feed: ~100% of departures due within 30
//! minutes carry a prediction, tailing off past 45. Beyond that we fall back to
//! the timetable, which is the right answer anyway.

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

const TRIP_UPDATES: &str =
    "https://nextrip-public-api.azure-api.net/octranspo/gtfs-rt-tp/beta/v1/TripUpdates";

/// Names accepted for the subscription key, in `.env` or the environment.
const KEY_NAMES: [&str; 4] = [
    "OC_TRANSPO_SUBSCRIPTION_KEY",
    "OCT_SUBSCRIPTION_KEY",
    "OCTRANSPO_SUBSCRIPTION_KEY",
    "SUBSCRIPTION_KEY",
];

/// How long a cached feed stays usable. Vehicles report roughly every 30s.
pub const TTL_SECS: i64 = 25;

#[derive(Default, Debug)]
pub struct Realtime {
    /// (trip_id, stop_id) -> predicted arrival, epoch seconds.
    arrivals: HashMap<(String, String), i64>,
    canceled: HashSet<String>,
    /// When the agency built the feed.
    pub feed_ts: i64,
    /// When we fetched it.
    pub fetched_at: i64,
    pub trips: usize,
}

impl Realtime {
    pub fn arrival(&self, trip_id: &str, stop_id: &str) -> Option<i64> {
        self.arrivals
            .get(&(trip_id.to_string(), stop_id.to_string()))
            .copied()
    }

    pub fn is_canceled(&self, trip_id: &str) -> bool {
        self.canceled.contains(trip_id)
    }

    /// Seconds since the agency stamped the feed.
    pub fn age(&self, now: i64) -> i64 {
        (now - self.feed_ts).max(0)
    }
}

pub fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Look for the subscription key in the environment, then `.env`.
pub fn find_key() -> Option<String> {
    for name in KEY_NAMES {
        if let Ok(v) = std::env::var(name) {
            let v = v.trim().to_string();
            if !v.is_empty() && v != "your_key_here" {
                return Some(v);
            }
        }
    }
    let mut candidates = vec![PathBuf::from(".env")];
    if let Some(d) = directories::BaseDirs::new() {
        candidates.push(d.home_dir().join(".config/otransit/.env"));
    }
    for path in candidates {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            if KEY_NAMES.contains(&k.trim()) {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                if !v.is_empty() && v != "your_key_here" {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

fn as_i64(v: Option<&Value>) -> Option<i64> {
    v.and_then(serde_json::Value::as_i64)
}

/// Parse the .NET-shaped TripUpdates payload.
pub fn parse(bytes: &[u8]) -> Result<Realtime> {
    let v: Value = serde_json::from_slice(bytes).context("parsing TripUpdates JSON")?;
    let mut rt = Realtime {
        fetched_at: now_epoch(),
        ..Default::default()
    };
    rt.feed_ts = as_i64(v.pointer("/Header/Timestamp")).unwrap_or(rt.fetched_at);

    let Some(entities) = v.get("Entity").and_then(|e| e.as_array()) else {
        bail!("TripUpdates payload had no Entity array");
    };

    for e in entities {
        let Some(tu) = e.get("TripUpdate") else {
            continue;
        };
        let Some(trip) = tu.get("Trip") else { continue };
        let Some(trip_id) = trip.get("TripId").and_then(|t| t.as_str()) else {
            continue;
        };
        rt.trips += 1;

        // 3 = CANCELED. The feed also emits 8, which isn't in the spec; it
        // marks added/unscheduled trips, so leave those alone.
        if as_i64(trip.get("ScheduleRelationship")) == Some(3) {
            rt.canceled.insert(trip_id.to_string());
        }

        let Some(stus) = tu.get("StopTimeUpdate").and_then(|s| s.as_array()) else {
            continue;
        };
        for s in stus {
            let Some(stop_id) = s.get("StopId").and_then(|x| x.as_str()) else {
                continue;
            };
            // Prefer Arrival; a handful of entries only carry Departure.
            let time = ["Arrival", "Departure"].iter().find_map(|side| {
                let node = s.get(*side)?;
                if node.get("HasTime").and_then(serde_json::Value::as_bool) == Some(true) {
                    as_i64(node.get("Time"))
                } else {
                    None
                }
            });
            if let Some(t) = time {
                rt.arrivals
                    .insert((trip_id.to_string(), stop_id.to_string()), t);
            }
        }
    }
    Ok(rt)
}

/// Fetch the raw payload. Public so `otransit probe` can report on the bytes
/// the agency actually sent, not just what we managed to parse from them.
pub fn fetch_raw(key: &str) -> Result<Vec<u8>> {
    let resp = ureq::get(TRIP_UPDATES)
        .query("format", "json")
        .set("Ocp-Apim-Subscription-Key", key)
        .timeout(std::time::Duration::from_secs(45))
        .call();
    let resp = match resp {
        Ok(r) => r,
        Err(ureq::Error::Status(401 | 403, _)) => {
            bail!("realtime feed rejected the key. Check it is subscribed to the product")
        }
        Err(ureq::Error::Status(code, r)) => {
            bail!("realtime feed returned HTTP {code} {}", r.status_text())
        }
        Err(e) => return Err(e).context("fetching TripUpdates"),
    };
    let mut buf = Vec::new();
    resp.into_reader().take(64 << 20).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Where the raw response is parked between runs.
fn cache_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join("trip_updates.json")
}

/// Fetch from the network and refresh the on-disk copy.
///
/// The polling loop calls this directly: the loop is itself the rate limiter,
/// so consulting the cache there would only blur the cadence.
pub fn refresh(cache_dir: &Path, key: &str) -> Result<Realtime> {
    let bytes = fetch_raw(key)?;
    let rt = parse(&bytes)?;
    // Write via a temp file so a concurrent reader never sees half a response.
    let path = cache_path(cache_dir);
    let tmp = path.with_extension("json.new");
    if std::fs::write(&tmp, &bytes).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
    Ok(rt)
}

/// Reuse the on-disk copy while it is inside the TTL, otherwise fetch.
///
/// Used for the first load of a process, so relaunching twice in quick
/// succession doesn't hit the agency twice. The agency asks callers to cache.
pub fn load(cache_dir: &Path, key: &str) -> Result<Realtime> {
    let path = cache_path(cache_dir);
    if let Ok(meta) = std::fs::metadata(&path) {
        let age = meta
            .modified()
            .ok()
            .and_then(|m| m.elapsed().ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(i64::MAX);
        if age <= TTL_SECS
            && let Ok(bytes) = std::fs::read(&path)
            && let Ok(rt) = parse(&bytes)
        {
            return Ok(rt);
        }
    }
    refresh(cache_dir, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestRt;

    const FEED_TS: i64 = 1_787_358_644;

    #[test]
    fn reads_an_arrival_prediction() {
        let rt = parse(&TestRt::new(FEED_TS).arrival("t1", "S1", 1000).build()).unwrap();
        assert_eq!(rt.arrival("t1", "S1"), Some(1000));
        assert_eq!(rt.trips, 1);
    }

    #[test]
    fn a_prediction_belongs_to_one_trip_and_stop_only() {
        let rt = parse(&TestRt::new(FEED_TS).arrival("t1", "S1", 1000).build()).unwrap();
        assert_eq!(rt.arrival("t1", "S2"), None, "wrong stop");
        assert_eq!(rt.arrival("t2", "S1"), None, "wrong trip");
    }

    #[test]
    fn falls_back_to_departure_when_there_is_no_arrival_time() {
        let rt = parse(
            &TestRt::new(FEED_TS)
                .departure_only("t1", "S1", 2000)
                .build(),
        )
        .unwrap();
        assert_eq!(rt.arrival("t1", "S1"), Some(2000));
    }

    #[test]
    fn ignores_a_time_whose_has_time_flag_is_false() {
        // The .NET serialisation always emits Time; only HasTime says whether
        // it means anything. Reading Time directly yields a garbage prediction.
        let rt = parse(&TestRt::new(FEED_TS).no_time("t1", "S1").build()).unwrap();
        assert_eq!(rt.arrival("t1", "S1"), None);
        assert_eq!(
            rt.trips, 1,
            "the trip is still present, just without a time"
        );
    }

    #[test]
    fn records_a_cancelled_trip() {
        let rt = parse(&TestRt::new(FEED_TS).canceled("t1").build()).unwrap();
        assert!(rt.is_canceled("t1"));
        assert_eq!(
            rt.arrival("t1", "S1"),
            None,
            "cancelled trips carry no times"
        );
    }

    #[test]
    fn schedule_relationship_8_is_not_a_cancellation() {
        // 8 is not in the GTFS-RT spec but the live feed emits it for added and
        // unscheduled trips, which carry real predictions. Treating any
        // non-zero relationship as cancelled would silently drop them.
        let rt = parse(&TestRt::new(FEED_TS).unscheduled("t1", "S1", 3000).build()).unwrap();
        assert!(!rt.is_canceled("t1"));
        assert_eq!(rt.arrival("t1", "S1"), Some(3000));
    }

    #[test]
    fn keeps_the_feed_timestamp_so_staleness_is_measurable() {
        let rt = parse(&TestRt::new(FEED_TS).arrival("t1", "S1", 1).build()).unwrap();
        assert_eq!(rt.feed_ts, FEED_TS);
        assert_eq!(rt.age(FEED_TS + 90), 90);
        assert_eq!(
            rt.age(FEED_TS - 5),
            0,
            "a clock skew must not read as negative"
        );
    }

    #[test]
    fn an_empty_feed_parses_and_reports_no_trips() {
        // A feed with no active trips and a feed we failed to parse look the
        // same on screen, so the count has to be observable.
        let rt = parse(&TestRt::new(FEED_TS).build()).unwrap();
        assert_eq!(rt.trips, 0);
    }

    #[test]
    fn a_payload_without_an_entity_array_is_an_error_not_an_empty_feed() {
        let err = parse(br#"{"Header":{"Timestamp":1}}"#).unwrap_err();
        assert!(
            err.to_string().contains("Entity"),
            "error should name what was missing, got: {err}"
        );
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse(b"not json").is_err());
    }

    #[test]
    fn mixed_feed_keeps_every_trip_distinct() {
        let bytes = TestRt::new(FEED_TS)
            .arrival("t1", "S1", 1000)
            .canceled("t2")
            .unscheduled("t3", "S1", 3000)
            .no_time("t4", "S1")
            .build();
        let rt = parse(&bytes).unwrap();
        assert_eq!(rt.trips, 4);
        assert_eq!(rt.arrival("t1", "S1"), Some(1000));
        assert!(rt.is_canceled("t2"));
        assert!(!rt.is_canceled("t3"));
        assert_eq!(rt.arrival("t3", "S1"), Some(3000));
        assert_eq!(rt.arrival("t4", "S1"), None);
    }
}
