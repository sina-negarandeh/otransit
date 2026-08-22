//! Which services run on a given date.

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use rusqlite::Connection;

/// service_ids running on a given date, honouring calendar_dates exceptions.
pub fn active_services(conn: &Connection, date: NaiveDate) -> Result<Vec<String>> {
    let col = match date.weekday() {
        chrono::Weekday::Mon => "mon",
        chrono::Weekday::Tue => "tue",
        chrono::Weekday::Wed => "wed",
        chrono::Weekday::Thu => "thu",
        chrono::Weekday::Fri => "fri",
        chrono::Weekday::Sat => "sat",
        chrono::Weekday::Sun => "sun",
    };
    let ds = date.format("%Y%m%d").to_string();

    let sql = format!(
        "SELECT service_id FROM calendar
          WHERE {col} = 1 AND start_date <= ?1 AND end_date >= ?1"
    );
    let mut set: std::collections::BTreeSet<String> = conn
        .prepare(&sql)?
        .query_map([&ds], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?;

    let mut st =
        conn.prepare("SELECT service_id, exception FROM calendar_dates WHERE date = ?1")?;
    let rows = st.query_map([&ds], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    for row in rows {
        let (sid, ex) = row?;
        if ex == 1 {
            set.insert(sid);
        } else {
            set.remove(&sid);
        }
    }
    Ok(set.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TestGtfs, svc};

    // ---------- active_services ----------

    #[test]
    fn a_service_runs_only_on_its_weekdays_and_inside_its_date_range() {
        let g = TestGtfs::new().service("WD", "1111100", "20260101", "20261231");
        let fri = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
        let sat = NaiveDate::from_ymd_opt(2026, 8, 22).unwrap();
        let before = NaiveDate::from_ymd_opt(2025, 12, 31).unwrap();

        assert_eq!(active_services(g.conn(), fri).unwrap(), svc(&["WD"]));
        assert!(
            active_services(g.conn(), sat).unwrap().is_empty(),
            "Saturday"
        );
        assert!(
            active_services(g.conn(), before).unwrap().is_empty(),
            "before start"
        );
    }

    #[test]
    fn a_calendar_exception_can_add_a_service_on_one_date() {
        // Holiday service: normally weekdays only, added for one Saturday.
        let g = TestGtfs::new()
            .service("WD", "1111100", "20260101", "20261231")
            .service_exception("WD", "20260822", 1);
        let sat = NaiveDate::from_ymd_opt(2026, 8, 22).unwrap();
        assert_eq!(active_services(g.conn(), sat).unwrap(), svc(&["WD"]));
    }

    #[test]
    fn a_calendar_exception_can_remove_a_service_on_one_date() {
        let g = TestGtfs::new()
            .service("WD", "1111100", "20260101", "20261231")
            .service_exception("WD", "20260821", 2);
        let fri = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
        assert!(active_services(g.conn(), fri).unwrap().is_empty());
    }
}
