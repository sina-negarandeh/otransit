//! Service-day arithmetic and clock formatting.
//!
//! GTFS counts from the start of the service day, not from midnight, and lets
//! `arrival_time` run past 24:00. Everything that turns those seconds into
//! something a person reads lives here.

use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone};

/// Start of a service day, as GTFS defines it: noon minus twelve hours.
///
/// That is midnight on all but two days a year. On the days the clocks change
/// it is 23:00 or 01:00 of the previous day, which is exactly what makes
/// `origin + arrival_time` land on the right wall clock either side of the
/// jump. Treating the origin as plain midnight shifts every trip after 02:00
/// by an hour on those days.
fn service_day_start(date: NaiveDate) -> DateTime<Local> {
    service_day_start_in(&Local, date)
}

/// The same, in a named zone.
///
/// The app always wants the machine's, but the behaviour this exists for only
/// happens in a zone that changes its clocks. A test that reads `Local` is
/// really asserting something about the machine it runs on: this one passed in
/// Ottawa for a session and failed the first time CI ran it in UTC.
fn service_day_start_in<Tz: TimeZone>(tz: &Tz, date: NaiveDate) -> DateTime<Tz> {
    let noon = date
        .and_hms_opt(12, 0, 0)
        .expect("noon is a valid time on every date");
    // Noon is never skipped or repeated by a DST transition, so this is
    // unambiguous even on changeover days.
    let noon = tz
        .from_local_datetime(&noon)
        .single()
        .unwrap_or_else(|| tz.from_local_datetime(&noon).earliest().expect("noon"));
    noon - Duration::hours(12)
}

/// How far `now` is into the service day, in seconds. Comparable directly with
/// GTFS `arrival_time`.
fn secs_into_service_day(now: DateTime<Local>, date: NaiveDate) -> i32 {
    i32::try_from((now - service_day_start(date)).num_seconds()).unwrap_or(i32::MAX)
}

pub(super) fn now_secs(date: NaiveDate) -> i32 {
    secs_into_service_day(Local::now(), date)
}

/// A realtime prediction (UTC epoch) onto the same axis as `Departure::secs`.
///
/// UTC-to-local is never ambiguous, unlike local-to-UTC, so `single()` always
/// resolves here even during the repeated hour at fall-back.
pub fn epoch_to_service_secs(epoch: i64, date: NaiveDate) -> Option<i32> {
    let dt = Local.timestamp_opt(epoch, 0).single()?;
    Some(secs_into_service_day(dt, date))
}

/// Minutes a trip is running late. Negative means early.
pub fn lateness(live: i32, scheduled: i32) -> i32 {
    let mut d = live - scheduled;
    // A prediction either side of midnight must not read as a 24h delay.
    if d > 43_200 {
        d -= 86_400;
    }
    if d < -43_200 {
        d += 86_400;
    }
    (d as f32 / 60.0).round() as i32
}

/// Format seconds-since-midnight as HH:MM, wrapping past 24h.
pub fn fmt_hm(secs: i32) -> String {
    let s = secs.rem_euclid(86_400);
    format!("{:02}:{:02}", s / 3600, (s % 3600) / 60)
}

/// Minutes from now, given both are seconds-since-midnight today.
pub fn mins_until(dep: i32, now: i32) -> i32 {
    let mut d = dep - now;
    if d < -3600 {
        d += 86_400;
    }
    (d as f32 / 60.0).round() as i32
}

/// Width of the wait column, in cells.
///
/// Two hour digits, because a board spans today plus yesterday's service and a
/// quiet stop can be sixteen hours from its next departure. Right-aligning
/// every form in this width is what lines their parts up: the minute units
/// digit lands in the same cell whether the string is "5 min" or "16h 40 min".
pub const WAIT_W: usize = 10;

/// "due", "7 min", "1h 26 min", "16h 40 min".
///
/// Deliberately not "1h26": this column sits beside actual clock times, and a
/// duration that looks like a time is a misread waiting to happen.
///
/// Past an hour the minutes are always spelled out and zero-padded, so every
/// hour form is the same shape. The board right-aligns this column, so any
/// string whose length follows its own contents drags the "h" along with it:
/// "1h 6 min" lands a cell right of "1h 36 min", and a bare "4h" is flung to
/// the far edge with a hole under the minutes.
pub fn fmt_wait(mins: i32) -> String {
    match mins {
        m if m <= 0 => "due".into(),
        m if m < 60 => format!("{m} min"),
        m => format!("{}h {:02} min", m / 60, m % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- the service day ----------

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn an_ordinary_service_day_starts_at_midnight() {
        let date = d(2026, 8, 22);
        let start = service_day_start(date);
        assert_eq!(start.date_naive(), date);
        assert_eq!(start.time(), chrono::NaiveTime::MIN);
    }

    #[test]
    fn arrival_times_land_on_the_right_wall_clock_on_an_ordinary_day() {
        let date = d(2026, 8, 22);
        let at_0800 = service_day_start(date) + Duration::seconds(8 * 3600);
        assert_eq!(at_0800.format("%H:%M").to_string(), "08:00");
        // And a 25:10 trip is 01:10 the next morning.
        let after_midnight = service_day_start(date) + Duration::seconds(25 * 3600 + 600);
        assert_eq!(after_midnight.format("%H:%M").to_string(), "01:10");
    }

    #[test]
    fn the_service_day_origin_shifts_when_the_clocks_change() {
        // GTFS measures arrival_time from noon minus twelve hours, which is
        // midnight on all but two days a year. Ottawa springs forward on
        // 2026-03-08 and falls back on 2026-11-01; on those days midnight and
        // noon are 11 and 13 real hours apart, so the origin moves.
        //
        // The zone is named rather than inherited. Reading the machine's would
        // make this assert something about where the suite runs: it passed here
        // and failed on a UTC runner, which has no transition on these dates.
        let tz = chrono_tz::America::Toronto;
        let spring = service_day_start_in(&tz, d(2026, 3, 8));
        let fall = service_day_start_in(&tz, d(2026, 11, 1));
        assert_ne!(
            spring.time(),
            chrono::NaiveTime::MIN,
            "spring-forward origin should not be midnight"
        );
        assert_ne!(
            fall.time(),
            chrono::NaiveTime::MIN,
            "fall-back origin should not be midnight"
        );
    }

    #[test]
    fn noon_is_twelve_hours_into_every_service_day_including_dst_days() {
        // The invariant the definition exists to preserve.
        for date in [d(2026, 8, 22), d(2026, 3, 8), d(2026, 11, 1)] {
            let noon = service_day_start(date) + Duration::hours(12);
            assert_eq!(
                noon.format("%H:%M").to_string(),
                "12:00",
                "{date} noon drifted"
            );
            assert_eq!(noon.date_naive(), date);
        }
    }

    #[test]
    fn now_and_scheduled_times_share_one_axis() {
        // secs_into_service_day is the inverse of adding seconds to the origin,
        // so `arr >= now` compares like with like.
        let date = d(2026, 3, 8);
        for secs in [0, 3600, 8 * 3600, 25 * 3600] {
            let moment = service_day_start(date) + Duration::seconds(secs);
            assert_eq!(
                secs_into_service_day(moment, date),
                i32::try_from(secs).unwrap()
            );
        }
    }

    #[test]
    fn a_live_prediction_maps_onto_the_same_axis() {
        let date = d(2026, 8, 22);
        let moment = service_day_start(date) + Duration::seconds(9 * 3600);
        assert_eq!(
            epoch_to_service_secs(moment.timestamp(), date),
            Some(9 * 3600)
        );
    }

    // ---------- lateness ----------

    #[test]
    fn lateness_is_signed_minutes_against_the_timetable() {
        assert_eq!(lateness(10 * 60, 10 * 60), 0);
        assert_eq!(lateness(10 * 60 + 300, 10 * 60), 5);
        assert_eq!(lateness(10 * 60 - 120, 10 * 60), -2);
    }

    #[test]
    fn a_prediction_across_midnight_is_not_a_twenty_four_hour_delay() {
        // Scheduled 23:58, predicted 00:03 the next day. Naive subtraction
        // reads that as running 1435 minutes early.
        let scheduled = 23 * 3600 + 58 * 60;
        let live = 3 * 60;
        assert_eq!(lateness(live, scheduled), 5);

        // And the mirror: scheduled just after midnight, running early.
        assert_eq!(lateness(23 * 3600 + 59 * 60, 2 * 60), -3);
    }

    // ---------- clock formatting ----------

    #[test]
    fn a_wait_never_looks_like_a_clock_time() {
        assert_eq!(fmt_wait(0), "due");
        assert_eq!(fmt_wait(-3), "due");
        assert_eq!(fmt_wait(27), "27 min");
        assert_eq!(fmt_wait(59), "59 min");
        assert_eq!(fmt_wait(86), "1h 26 min");
        assert_eq!(fmt_wait(195), "3h 15 min");
        // Past an hour the minutes are always spelled out, zero-padded. "4h"
        // on its own leaves a hole under the minutes column, and a whole hour
        // is not special enough to earn a shape of its own.
        assert_eq!(fmt_wait(66), "1h 06 min");
        assert_eq!(fmt_wait(60), "1h 00 min");
        assert_eq!(fmt_wait(240), "4h 00 min");
        // No output may contain a colon, or it reads as a departure time.
        for m in 0..600 {
            assert!(!fmt_wait(m).contains(':'), "{m} formatted with a colon");
        }
    }

    #[test]
    fn no_wait_outgrows_the_column_it_is_poured_into() {
        // The board pads to WAIT_W and never truncates, so one overlong string
        // pushes every column after it right on that row alone. Stop #7041 has
        // a 16h gap in today's feed, so this is ordinary, not a corner case.
        // A board spans at most today+yesterday service, so waits stay under
        // 48h and two hour digits are enough.
        for mins in [0, 1, 59, 60, 599, 600, 601, 1439, 1440, 2879] {
            let s = fmt_wait(mins);
            assert!(
                s.chars().count() <= WAIT_W,
                "fmt_wait({mins}) = {s:?} is {} cells, the column is {WAIT_W}",
                s.chars().count()
            );
        }
    }

    #[test]
    fn a_column_of_waits_keeps_its_parts_in_one_place() {
        // The board right-aligns this column, so a string whose length depends
        // on its minute digits drags the "h" with it: "1h 6 min" lands one cell
        // right of "1h 36 min" and the h's stagger all the way down.
        let column: Vec<String> = [5, 25, 36, 60, 66, 96, 120, 126, 195, 240, 714, 1439]
            .iter()
            .map(|m| format!("{:>WAIT_W$}", fmt_wait(*m)))
            .collect();

        let same = |name: &str, cols: Vec<usize>| {
            assert!(
                cols.windows(2).all(|w| w[0] == w[1]),
                "the {name} column staggers: {column:#?}"
            );
        };
        same("h", column.iter().filter_map(|s| s.find('h')).collect());
        same(
            "min",
            column.iter().filter_map(|s| s.find(" min")).collect(),
        );
        for row in &column {
            assert_eq!(row.chars().count(), WAIT_W, "{row:?} is not one cell wide");
        }
    }

    #[test]
    fn times_past_midnight_display_on_a_normal_clock() {
        assert_eq!(fmt_hm(25 * 3600 + 10 * 60), "01:10");
        assert_eq!(fmt_hm(0), "00:00");
        assert_eq!(fmt_hm(23 * 3600 + 59 * 60), "23:59");
    }

    #[test]
    fn minutes_until_handles_a_departure_that_wrapped_past_midnight() {
        // now 23:50, departure 00:05 -> 15 minutes, not -1425.
        assert_eq!(mins_until(5 * 60, 23 * 3600 + 50 * 60), 15);
    }

    #[test]
    fn a_departure_just_gone_reads_as_negative_not_as_tomorrow() {
        assert_eq!(mins_until(10 * 3600, 10 * 3600 + 120), -2);
    }
}
