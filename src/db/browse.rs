//! Walking down the hierarchy: routes, then directions, then stops, then the
//! board for one stop.

use super::{Departure, Direction, Route, ServiceDay, StopRow, natural_key, placeholders};
use anyhow::Result;
use rusqlite::{Connection, params_from_iter};

/// Level 1: routes of a given type (0 = O-Train, 3 = bus) running on `date`.
pub fn routes_for_type(
    conn: &Connection,
    route_type: i64,
    services: &[String],
) -> Result<Vec<Route>> {
    if services.is_empty() {
        return Ok(vec![]);
    }
    let sql = format!(
        "SELECT r.short_name,
                MIN(r.long_name), MIN(r.color),
                GROUP_CONCAT(r.route_id, '\x1f')
           FROM routes r
          WHERE r.route_type = ?
            AND EXISTS (SELECT 1 FROM trips t
                         WHERE t.route_id = r.route_id
                           AND t.service_id IN ({}))
          GROUP BY r.short_name",
        placeholders(services.len())
    );
    let mut st = conn.prepare(&sql)?;
    let mut args: Vec<String> = vec![route_type.to_string()];
    args.extend(services.iter().cloned());
    let mut out: Vec<Route> = st
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(Route {
                short_name: r.get(0)?,
                long_name: r.get(1)?,
                color: r.get(2)?,
                route_ids: r
                    .get::<_, String>(3)?
                    .split('\x1f')
                    .map(std::string::ToString::to_string)
                    .collect(),
                route_type,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    out.sort_by_key(|r| natural_key(&r.short_name));
    Ok(out)
}

/// The routes running today under these short names, whatever their mode.
///
/// The sibling of `stops_by_id`, and there for the same caller: a pin stores a
/// route's short name, and the name has to be resolved against the current
/// export because `update` replaces the database wholesale.
///
/// Grouped by short name *and* route type. Grouping by name alone would merge
/// a bus and a rail line that shared one into a single row with both sets of
/// route_ids, which is a wrong answer rather than an ambiguous one. Two rows
/// come back instead, and the caller decides what to do about it.
pub fn routes_by_short_name(
    conn: &Connection,
    names: &[String],
    services: &[String],
) -> Result<Vec<Route>> {
    if names.is_empty() || services.is_empty() {
        return Ok(vec![]);
    }
    let sql = format!(
        "SELECT r.short_name, r.route_type,
                MIN(r.long_name), MIN(r.color),
                GROUP_CONCAT(r.route_id, '\x1f')
           FROM routes r
          WHERE r.short_name IN ({})
            AND EXISTS (SELECT 1 FROM trips t
                         WHERE t.route_id = r.route_id
                           AND t.service_id IN ({}))
          GROUP BY r.short_name, r.route_type",
        placeholders(names.len()),
        placeholders(services.len())
    );
    let mut st = conn.prepare(&sql)?;
    let mut args: Vec<String> = names.to_vec();
    args.extend(services.iter().cloned());
    Ok(st
        .query_map(params_from_iter(args.iter()), |r| {
            Ok(Route {
                short_name: r.get(0)?,
                route_type: r.get(1)?,
                long_name: r.get(2)?,
                color: r.get(3)?,
                route_ids: r
                    .get::<_, String>(4)?
                    .split('\x1f')
                    .map(std::string::ToString::to_string)
                    .collect(),
            })
        })?
        .collect::<std::result::Result<_, _>>()?)
}

/// Level 2: the distinct headsigns for a route, most-used first.
pub fn directions_for_route(
    conn: &Connection,
    route_ids: &[String],
    services: &[String],
) -> Result<Vec<Direction>> {
    if route_ids.is_empty() || services.is_empty() {
        return Ok(vec![]);
    }
    let sql = format!(
        "SELECT t.headsign, COUNT(*) c
           FROM trips t
          WHERE t.route_id IN ({}) AND t.service_id IN ({})
          GROUP BY t.headsign
          ORDER BY c DESC",
        placeholders(route_ids.len()),
        placeholders(services.len())
    );
    let mut st = conn.prepare(&sql)?;
    let args: Vec<&String> = route_ids.iter().chain(services.iter()).collect();
    let out = st
        .query_map(params_from_iter(args), |r| {
            Ok(Direction {
                headsign: r.get(0)?,
                trips: r.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(out)
}

/// Level 3: stops along a route+headsign, in travel order.
///
/// Uses the longest trip as the canonical pattern, so short-turns don't hide stops.
pub fn stops_for_direction(
    conn: &Connection,
    route_ids: &[String],
    services: &[String],
    headsign: &str,
) -> Result<Vec<StopRow>> {
    if route_ids.is_empty() || services.is_empty() {
        return Ok(vec![]);
    }
    let sql = format!(
        "SELECT t.trip_id, COUNT(st.stop_id) c
           FROM trips t JOIN stop_times st ON st.trip_id = t.trip_id
          WHERE t.route_id IN ({}) AND t.service_id IN ({}) AND t.headsign = ?
          GROUP BY t.trip_id ORDER BY c DESC LIMIT 1",
        placeholders(route_ids.len()),
        placeholders(services.len())
    );
    let mut args: Vec<String> = route_ids.to_vec();
    args.extend(services.iter().cloned());
    args.push(headsign.to_string());

    let trip_id: Option<String> = conn
        .prepare(&sql)?
        .query_map(params_from_iter(args.iter()), |r| r.get::<_, String>(0))?
        .next()
        .transpose()?;
    let Some(trip_id) = trip_id else {
        return Ok(vec![]);
    };

    let mut st = conn.prepare(
        "SELECT s.stop_id, s.stop_code, s.name
           FROM stop_times st JOIN stops s ON s.stop_id = st.stop_id
          WHERE st.trip_id = ?1 ORDER BY st.seq",
    )?;
    let out = st
        .query_map([&trip_id], |r| {
            Ok(StopRow {
                stop_id: r.get(0)?,
                code: r.get(1)?,
                name: r.get(2)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(out)
}

/// Which departures a board wants.
///
/// A named pair of cases rather than a nullable tuple: the caller reads as
/// prose, and each variant builds its own SQL fragment and arguments in one
/// place, so the placeholder count cannot drift from the argument list.
#[derive(Clone, Copy)]
pub enum Narrow<'a> {
    /// Everything calling at the stop, for a board reached by search.
    Everything,
    /// One route in one direction, for a board reached by drilling down.
    Route {
        route_ids: &'a [String],
        headsign: &'a str,
    },
}

impl Narrow<'_> {
    /// The extra WHERE clause, and the arguments it needs, together.
    fn sql_and_args(&self) -> (String, Vec<String>) {
        match self {
            Narrow::Everything => (String::new(), vec![]),
            Narrow::Route {
                route_ids,
                headsign,
            } => {
                let mut args: Vec<String> = route_ids.to_vec();
                args.push((*headsign).to_string());
                (
                    format!(
                        "AND t.route_id IN ({}) AND t.headsign = ?",
                        placeholders(route_ids.len())
                    ),
                    args,
                )
            }
        }
    }
}

/// Level 4: upcoming departures at a stop.
///
/// Runs twice: once for today's services, once for yesterday's, because a trip
/// scheduled at 25:10 on Friday is what you catch at 01:10 on Saturday.
/// The stops behind a list of pinned ids, in the order asked for.
///
/// Ids absent from the cache are simply missing from the result: `update`
/// replaces the whole database, so a pinned stop can vanish between exports,
/// and a pin pointing at one is hidden rather than shown as a row that cannot
/// be opened. The stored line stays, so a stop that comes back brings its pin
/// with it.
pub fn stops_by_id(conn: &Connection, ids: &[String]) -> Result<Vec<StopRow>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let sql = format!(
        "SELECT stop_id, stop_code, name FROM stops WHERE stop_id IN ({})",
        placeholders(ids.len())
    );
    let mut st = conn.prepare(&sql)?;
    let found: Vec<StopRow> = st
        .query_map(params_from_iter(ids.iter()), |r| {
            Ok(StopRow {
                stop_id: r.get(0)?,
                code: r.get(1)?,
                name: r.get(2)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    // SQL has no order to give back, and the order is the whole point: a pin's
    // position in the list is muscle memory.
    Ok(ids
        .iter()
        .filter_map(|id| found.iter().find(|s| &s.stop_id == id).cloned())
        .collect())
}

pub fn departures(
    conn: &Connection,
    stop_id: &str,
    narrow: Narrow,
    day: &ServiceDay,
    limit: usize,
) -> Result<Vec<Departure>> {
    let mut out: Vec<Departure> = Vec::new();

    for (services, shift) in [(day.today, 0i32), (day.yesterday, 86_400i32)] {
        if services.is_empty() {
            continue;
        }
        let (narrow_sql, narrow_args) = narrow.sql_and_args();
        let sql = format!(
            "SELECT st.arr, t.trip_id, r.short_name, r.color, t.headsign
               FROM stop_times st
               JOIN trips t  ON t.trip_id = st.trip_id
               JOIN routes r ON r.route_id = t.route_id
              WHERE st.stop_id = ?
                AND t.service_id IN ({})
                AND st.arr >= ?
                {narrow_sql}
              ORDER BY st.arr LIMIT ?",
            placeholders(services.len())
        );

        let mut args: Vec<String> = vec![stop_id.to_string()];
        args.extend(services.iter().cloned());
        args.push((day.now + shift).to_string());
        args.extend(narrow_args.iter().cloned());
        args.push(limit.to_string());

        let mut st = conn.prepare(&sql)?;
        let rows = st.query_map(params_from_iter(args.iter()), |r| {
            Ok((
                r.get::<_, i32>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        for row in rows {
            let (arr, trip_id, route_short, route_color, headsign) = row?;
            out.push(Departure {
                secs: arr - shift,
                trip_id,
                route_short,
                route_color,
                headsign,
                after_midnight: shift > 0,
                live: None,
                canceled: false,
            });
        }
    }
    out.sort_by_key(|d| d.secs);
    out.truncate(limit);
    Ok(out)
}

/// Order a board by when a bus actually arrives, not when it was meant to.
///
/// `departures` can only sort by the timetable, because live predictions are
/// attached afterwards. Without this a trip running ten minutes late sorts
/// ahead of one that is on time, and the board silently stops being in order.
pub fn sort_by_actual_arrival(deps: &mut [Departure]) {
    deps.sort_by_key(Departure::when);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TestGtfs, svc};

    fn day<'a>(today: &'a [String], yesterday: &'a [String], now: i32) -> ServiceDay<'a> {
        ServiceDay {
            today,
            yesterday,
            now,
        }
    }

    fn at(h: i32, m: i32) -> i32 {
        h * 3600 + m * 60
    }

    // ---------- booking periods ----------

    #[test]
    fn a_route_published_once_per_booking_period_appears_once() {
        // OC Transpo ships route 7 as route_id "7" and "7-1", one per booking
        // period. Without grouping, every route shows up twice.
        let g = TestGtfs::new()
            .route("7", "7", 3, "0057B8")
            .route("7-1", "7", 3, "0057B8")
            .always("A")
            .always("B")
            .trip("t-a", "7", "A", "St-Laurent")
            .trip("t-b", "7-1", "B", "St-Laurent");

        let routes = routes_for_type(g.conn(), 3, &svc(&["A", "B"])).unwrap();
        assert_eq!(routes.len(), 1, "one route, not two");
        assert_eq!(routes[0].short_name, "7");
        assert_eq!(routes[0].route_ids.len(), 2, "both ids kept for querying");
    }

    #[test]
    fn only_the_booking_period_running_today_is_kept() {
        let g = TestGtfs::new()
            .route("7", "7", 3, "0057B8")
            .route("7-1", "7", 3, "0057B8")
            .always("A")
            .always("B")
            .trip("t-a", "7", "A", "St-Laurent")
            .trip("t-b", "7-1", "B", "St-Laurent");

        let routes = routes_for_type(g.conn(), 3, &svc(&["A"])).unwrap();
        assert_eq!(routes[0].route_ids, vec!["7".to_string()]);
    }

    #[test]
    fn routes_sort_numerically_not_lexically() {
        let g = TestGtfs::new()
            .route("10", "10", 3, "x")
            .route("7", "7", 3, "x")
            .route("101", "101", 3, "x")
            .always("A")
            .trip("t1", "10", "A", "h")
            .trip("t2", "7", "A", "h")
            .trip("t3", "101", "A", "h");
        let names: Vec<String> = routes_for_type(g.conn(), 3, &svc(&["A"]))
            .unwrap()
            .into_iter()
            .map(|r| r.short_name)
            .collect();
        assert_eq!(names, vec!["7", "10", "101"]);
    }

    #[test]
    fn routes_of_another_type_are_not_returned() {
        let g = TestGtfs::new()
            .route("1", "1", 0, "D30F1D") // O-Train
            .route("7", "7", 3, "0057B8") // bus
            .always("A")
            .trip("t1", "1", "A", "Blair")
            .trip("t2", "7", "A", "St-Laurent");
        let rail = routes_for_type(g.conn(), 0, &svc(&["A"])).unwrap();
        assert_eq!(rail.len(), 1);
        assert_eq!(rail[0].short_name, "1");
    }

    // ---------- after midnight ----------

    /// A trip scheduled 25:10 on Friday is what you catch at 01:10 on Saturday.
    /// It belongs to Friday's service day and must appear on Saturday's board.
    #[test]
    fn a_trip_scheduled_past_midnight_appears_on_the_next_day() {
        let g = TestGtfs::new()
            .route("5", "5", 3, "x")
            .always("FRI")
            .trip("late", "5", "FRI", "Elmvale")
            .stop("S1", "0001", "WALLER")
            .stop_time("late", "S1", 1, "25:10:00");

        let today = svc(&[]); // Saturday has no service of its own here
        let yesterday = svc(&["FRI"]);
        let deps = departures(
            g.conn(),
            "S1",
            Narrow::Everything,
            &day(&today, &yesterday, at(1, 0)),
            10,
        )
        .unwrap();

        assert_eq!(deps.len(), 1, "the 25:10 trip must be reachable at 01:00");
        assert_eq!(deps[0].secs, at(1, 10), "normalised into today's clock");
        assert!(deps[0].after_midnight);
    }

    #[test]
    fn an_after_midnight_trip_that_has_already_gone_is_not_shown() {
        let g = TestGtfs::new()
            .route("5", "5", 3, "x")
            .always("FRI")
            .trip("late", "5", "FRI", "Elmvale")
            .stop("S1", "0001", "WALLER")
            .stop_time("late", "S1", 1, "25:10:00");

        let today = svc(&[]);
        let yesterday = svc(&["FRI"]);
        // 01:30 is past the 01:10 departure.
        let deps = departures(
            g.conn(),
            "S1",
            Narrow::Everything,
            &day(&today, &yesterday, at(1, 30)),
            10,
        )
        .unwrap();
        assert!(deps.is_empty());
    }

    // ---------- departures ----------

    #[test]
    fn departures_are_ordered_by_time_and_bounded_by_the_limit() {
        let g = TestGtfs::new()
            .route("5", "5", 3, "x")
            .always("A")
            .trip("t1", "5", "A", "h")
            .trip("t2", "5", "A", "h")
            .trip("t3", "5", "A", "h")
            .stop("S1", "0001", "STOP")
            .stop_time("t3", "S1", 1, "10:30:00")
            .stop_time("t1", "S1", 1, "10:10:00")
            .stop_time("t2", "S1", 1, "10:20:00");

        let today = svc(&["A"]);
        let none = svc(&[]);
        let deps = departures(
            g.conn(),
            "S1",
            Narrow::Everything,
            &day(&today, &none, at(9, 0)),
            2,
        )
        .unwrap();
        assert_eq!(deps.len(), 2, "limit respected");
        assert_eq!(deps[0].secs, at(10, 10));
        assert_eq!(deps[1].secs, at(10, 20));
    }

    #[test]
    fn a_stop_board_returns_every_route_calling_there() {
        let g = TestGtfs::new()
            .route("5", "5", 3, "x")
            .route("7", "7", 3, "y")
            .always("A")
            .trip("t5", "5", "A", "Elmvale")
            .trip("t7", "7", "A", "St-Laurent")
            .stop("S1", "0001", "STOP")
            .stop_time("t5", "S1", 1, "10:10:00")
            .stop_time("t7", "S1", 1, "10:15:00");

        let today = svc(&["A"]);
        let none = svc(&[]);
        let deps = departures(
            g.conn(),
            "S1",
            Narrow::Everything,
            &day(&today, &none, at(9, 0)),
            10,
        )
        .unwrap();
        let routes: Vec<&str> = deps.iter().map(|d| d.route_short.as_str()).collect();
        assert_eq!(routes, vec!["5", "7"]);
        assert_eq!(
            deps[0].headsign, "Elmvale",
            "each row carries its own headsign"
        );
    }

    #[test]
    fn a_filtered_board_returns_only_the_chosen_route_and_direction() {
        let g = TestGtfs::new()
            .route("5", "5", 3, "x")
            .route("7", "7", 3, "y")
            .always("A")
            .trip("t5", "5", "A", "Elmvale")
            .trip("t7", "7", "A", "St-Laurent")
            .trip("t5b", "5", "A", "Waller") // same route, other direction
            .stop("S1", "0001", "STOP")
            .stop_time("t5", "S1", 1, "10:10:00")
            .stop_time("t7", "S1", 1, "10:15:00")
            .stop_time("t5b", "S1", 1, "10:20:00");

        let today = svc(&["A"]);
        let none = svc(&[]);
        let ids = vec!["5".to_string()];
        let deps = departures(
            g.conn(),
            "S1",
            Narrow::Route {
                route_ids: &ids,
                headsign: "Elmvale",
            },
            &day(&today, &none, at(9, 0)),
            10,
        )
        .unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].trip_id, "t5");
    }

    // ---------- ordering once live data lands ----------

    fn dep(secs: i32, live: Option<i32>) -> Departure {
        Departure {
            secs,
            trip_id: format!("t{secs}"),
            route_short: "5".into(),
            route_color: "x".into(),
            headsign: "h".into(),
            after_midnight: false,
            live,
            canceled: false,
        }
    }

    #[test]
    fn a_board_is_ordered_by_actual_arrival_not_by_the_timetable() {
        // The 10:00 bus is running 12 minutes late, so the 10:05 one that is
        // on time now arrives first. Sorting on the schedule shows them the
        // wrong way round while displaying live times.
        let mut deps = vec![
            dep(at(10, 0), Some(at(10, 12))),
            dep(at(10, 5), Some(at(10, 5))),
        ];
        sort_by_actual_arrival(&mut deps);
        assert_eq!(deps[0].secs, at(10, 5), "the one actually arriving first");
        assert_eq!(deps[1].secs, at(10, 0));
    }

    #[test]
    fn rows_without_a_prediction_are_ordered_by_their_scheduled_time() {
        let mut deps = vec![dep(at(10, 30), None), dep(at(10, 10), None)];
        sort_by_actual_arrival(&mut deps);
        assert_eq!(deps[0].secs, at(10, 10));
    }

    #[test]
    fn live_and_scheduled_rows_interleave_correctly() {
        let mut deps = vec![
            dep(at(10, 0), Some(at(10, 20))), // late
            dep(at(10, 10), None),            // no prediction
        ];
        sort_by_actual_arrival(&mut deps);
        assert_eq!(
            deps[0].secs,
            at(10, 10),
            "scheduled 10:10 beats a late 10:20"
        );
    }

    // ---------- stops along a direction ----------

    #[test]
    fn the_stop_list_uses_the_longest_trip_so_short_turns_hide_nothing() {
        let g = TestGtfs::new()
            .route("5", "5", 3, "x")
            .always("A")
            .trip("full", "5", "A", "Elmvale")
            .trip("short", "5", "A", "Elmvale")
            .stop("S1", "0001", "FIRST")
            .stop("S2", "0002", "MIDDLE")
            .stop("S3", "0003", "LAST")
            .stop_time("full", "S1", 1, "10:00:00")
            .stop_time("full", "S2", 2, "10:05:00")
            .stop_time("full", "S3", 3, "10:10:00")
            .stop_time("short", "S1", 1, "11:00:00")
            .stop_time("short", "S2", 2, "11:05:00");

        let ids = vec!["5".to_string()];
        let stops = stops_for_direction(g.conn(), &ids, &svc(&["A"]), "Elmvale").unwrap();
        let names: Vec<&str> = stops.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["FIRST", "MIDDLE", "LAST"], "in travel order");
    }

    #[test]
    fn a_board_narrowed_to_no_routes_returns_nothing_rather_than_a_sql_error() {
        // SQLite reads `IN ()` as simply false rather than rejecting it, so
        // this is a query with no answer. Pinned because the obvious reading
        // is that it should be a syntax error.
        let g = TestGtfs::new()
            .route("5", "5", 3, "x")
            .always("A")
            .trip("t5", "5", "A", "Elmvale")
            .stop("S1", "0001", "STOP")
            .stop_time("t5", "S1", 1, "10:00:00");
        let today = svc(&["A"]);
        let none = svc(&[]);
        let empty: Vec<String> = vec![];
        let deps = departures(
            g.conn(),
            "S1",
            Narrow::Route {
                route_ids: &empty,
                headsign: "Elmvale",
            },
            &day(&today, &none, at(9, 0)),
            10,
        )
        .expect("an empty route list is a query with no answer, not an error");
        assert!(deps.is_empty());
    }

    #[test]
    fn an_empty_slice_returns_nothing_rather_than_a_sql_error() {
        // Nothing runs today, so nothing can be listed. The early return also
        // saves preparing a query whose `IN ()` can only match nothing.
        let g = TestGtfs::new();
        let none: Vec<String> = vec![];
        let ids = vec!["7".to_string()];
        assert!(
            directions_for_route(g.conn(), &ids, &none)
                .unwrap()
                .is_empty()
        );
        assert!(
            directions_for_route(g.conn(), &none, &svc(&["A"]))
                .unwrap()
                .is_empty()
        );
        assert!(
            stops_for_direction(g.conn(), &ids, &none, "x")
                .unwrap()
                .is_empty()
        );
        assert!(
            stops_for_direction(g.conn(), &none, &svc(&["A"]), "x")
                .unwrap()
                .is_empty()
        );
    }
}
