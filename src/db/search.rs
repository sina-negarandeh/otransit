//! Finding a stop by name or pole number, from the first screen.

use super::{StopHit, natural_key, placeholders};
use anyhow::Result;
use rusqlite::{Connection, params_from_iter};

/// What calls at a stop today: the routes, and the dominant destination.
fn stop_summary(
    conn: &Connection,
    stop_id: &str,
    services: &[String],
) -> Result<(Vec<String>, String)> {
    let sql = format!(
        "SELECT r.short_name, t.headsign, COUNT(*) c
           FROM stop_times st
           JOIN trips t  ON t.trip_id = st.trip_id
           JOIN routes r ON r.route_id = t.route_id
          WHERE st.stop_id = ? AND t.service_id IN ({})
          GROUP BY r.short_name, t.headsign",
        placeholders(services.len())
    );
    let mut args: Vec<String> = vec![stop_id.to_string()];
    args.extend(services.iter().cloned());
    let mut st = conn.prepare(&sql)?;
    let rows: Vec<(String, String, i64)> = st
        .query_map(params_from_iter(args.iter()), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<std::result::Result<_, _>>()?;

    let mut names: Vec<String> = rows.iter().map(|(n, _, _)| n.clone()).collect();
    names.sort_by_key(|s| natural_key(s));
    names.dedup();

    let mut by_headsign: std::collections::HashMap<&str, i64> = std::collections::HashMap::new();
    for (_, h, c) in &rows {
        *by_headsign.entry(h.as_str()).or_default() += c;
    }
    // Ties broken by name so the label doesn't flicker between runs.
    let toward = by_headsign
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(h, _)| h.to_string())
        .unwrap_or_default();
    Ok((names, toward))
}

/// Drop the platform designator from a stop name once it's shown separately,
/// so "RIDEAU C" with platform "C" reads as "RIDEAU".
///
/// Driven by the feed's platform_code, never by pattern-matching the name:
/// names like "VANTAGE / AD. 303" and "ROCKDALE / HIGHWAY 417" look exactly
/// like platform suffixes and are not.
pub fn strip_platform(name: &str, platform: &str) -> String {
    if platform.is_empty() {
        return name.to_string();
    }
    // Require whitespace before the designator. Without it a platform code
    // that happens to end a word is eaten from inside the name: "...AVENUE"
    // with platform "E" would render "...AVENU".
    if let Some(base) = name.strip_suffix(platform) {
        if base.ends_with(char::is_whitespace) {
            let base = base.trim_end();
            if !base.is_empty() {
                return base.to_string();
            }
        }
    }
    // O-Train platforms are coded "1"/"2" but named "... O-TRAIN EAST / EST".
    // The direction is already carried by the destination column.
    match name.split_once(" O-TRAIN ") {
        Some((base, _)) if !base.is_empty() => base.to_string(),
        _ => name.to_string(),
    }
}

/// Escape the characters LIKE treats as wildcards, so a query means what the
/// user typed. Without this, `%` matches everything and `_` matches any char.
fn like_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Search stops by pole number or name, best matches first.
///
/// Ranking happens in SQL rather than in Rust so that `LIMIT` applies to
/// *ranked* rows. Filtering first and ranking afterwards means an unordered
/// cap throws away the best matches before anything can score them: "st"
/// matches 1207 stops, and a 400-row cap dropped 195 of the 283 whose name
/// actually starts with "st".
///
/// Rank 0 is an exact pole number, 1 a pole-number prefix, 2 a name prefix,
/// 3 a name containing every token in any order, so "bank somerset" finds
/// "BANK / SOMERSET W".
///
/// Only stops with service today are returned: a platform nothing calls at is
/// noise, not an answer.
pub fn search_stops(
    conn: &Connection,
    query: &str,
    services: &[String],
    limit: usize,
) -> Result<Vec<StopHit>> {
    let q = query.trim().to_lowercase();
    if q.is_empty() || services.is_empty() {
        return Ok(vec![]);
    }

    let tokens: Vec<String> = q.split_whitespace().map(like_escape).collect();
    let whole = like_escape(&q);

    // Every token must appear in the name; or the code contains the query.
    let name_match = tokens
        .iter()
        .map(|_| "name_l LIKE ? ESCAPE '\\'")
        .collect::<Vec<_>>()
        .join(" AND ");

    let sql = format!(
        "WITH c AS (
             SELECT stop_id, stop_code, name, COALESCE(platform, '') AS platform,
                    LOWER(name) AS name_l, LOWER(stop_code) AS code_l
               FROM stops WHERE location_type != 1)
         SELECT stop_id, stop_code, name, platform,
                CASE WHEN code_l = ?                     THEN 0
                     WHEN code_l LIKE ? ESCAPE '\\'      THEN 1
                     WHEN name_l LIKE ? ESCAPE '\\'      THEN 2
                     ELSE 3 END AS rank
           FROM c
          WHERE ({name_match}) OR code_l LIKE ? ESCAPE '\\'
          ORDER BY rank, name
          LIMIT ?"
    );

    let mut args: Vec<String> = vec![
        q.clone(),           // exact code
        format!("{whole}%"), // code prefix
        format!("{whole}%"), // name prefix
    ];
    args.extend(tokens.iter().map(|t| format!("%{t}%")));
    args.push(format!("%{whole}%")); // code contains
    // Stops with no service today are dropped after the fact, so look past
    // `limit` before giving up rather than scanning everything.
    args.push((limit * 4).to_string());

    let mut st = conn.prepare(&sql)?;
    let ranked: Vec<(String, String, String, String)> = st
        .query_map(params_from_iter(args.iter()), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<std::result::Result<_, _>>()?;

    let mut out: Vec<StopHit> = Vec::new();
    for (stop_id, code, name, platform) in ranked {
        if out.len() >= limit {
            break;
        }
        let (routes, toward) = stop_summary(conn, &stop_id, services)?;
        if routes.is_empty() {
            continue;
        }
        out.push(StopHit {
            stop_id,
            code,
            name,
            platform,
            routes,
            toward,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TestGtfs, svc};

    // ---------- search ----------

    fn searchable() -> TestGtfs {
        TestGtfs::new()
            .route("7", "7", 3, "x")
            .always("A")
            .trip("t1", "7", "A", "St-Laurent")
            .trip("t2", "7", "A", "Carleton")
            .stop("bank", "1902", "BANK / SOMERSET W")
            .stop("somerset", "1548", "SOMERSET W / BANK")
            .stop_on_platform("rideau-a", "3009", "RIDEAU A", "A")
            .stop_on_platform("rideau-b", "3009", "RIDEAU B", "B")
            .stop("orphan", "9999", "NOBODY STOPS HERE")
            .stop_time("t1", "bank", 1, "10:00:00")
            .stop_time("t1", "somerset", 2, "10:05:00")
            .stop_time("t1", "rideau-a", 3, "10:10:00")
            .stop_time("t2", "rideau-b", 1, "09:00:00")
    }

    #[test]
    fn multi_token_search_matches_tokens_in_any_order() {
        // The SQL prefilter once used the whole query as one literal substring,
        // so "bank somerset" never matched "BANK / SOMERSET W" and the token
        // matching was unreachable.
        let g = searchable();
        let hits = search_stops(g.conn(), "bank somerset", &svc(&["A"]), 10).unwrap();
        let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"BANK / SOMERSET W"), "got {names:?}");
        assert!(
            names.contains(&"SOMERSET W / BANK"),
            "order must not matter"
        );
    }

    #[test]
    fn searching_a_pole_number_finds_every_platform_sharing_it() {
        let g = searchable();
        let hits = search_stops(g.conn(), "3009", &svc(&["A"]), 10).unwrap();
        assert_eq!(hits.len(), 2);
        let platforms: Vec<&str> = hits.iter().map(|h| h.platform.as_str()).collect();
        assert!(platforms.contains(&"A") && platforms.contains(&"B"));
    }

    #[test]
    fn a_stop_with_no_service_today_is_not_a_search_result() {
        let g = searchable();
        let hits = search_stops(g.conn(), "nobody", &svc(&["A"]), 10).unwrap();
        assert!(
            hits.is_empty(),
            "a platform nothing calls at is not an answer"
        );
    }

    #[test]
    fn search_results_carry_the_routes_and_direction_that_tell_platforms_apart() {
        let g = searchable();
        let hits = search_stops(g.conn(), "3009", &svc(&["A"]), 10).unwrap();
        let a = hits.iter().find(|h| h.platform == "A").unwrap();
        let b = hits.iter().find(|h| h.platform == "B").unwrap();
        assert_eq!(a.routes, vec!["7".to_string()]);
        assert_eq!(a.toward, "St-Laurent");
        assert_eq!(b.toward, "Carleton", "opposite sides differ by destination");
    }

    #[test]
    fn an_empty_query_returns_nothing_rather_than_everything() {
        let g = searchable();
        assert!(
            search_stops(g.conn(), "", &svc(&["A"]), 10)
                .unwrap()
                .is_empty()
        );
        assert!(
            search_stops(g.conn(), "   ", &svc(&["A"]), 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn search_respects_the_limit() {
        let g = searchable();
        let hits = search_stops(g.conn(), "3009", &svc(&["A"]), 1).unwrap();
        assert_eq!(hits.len(), 1);
    }

    // ---------- ranking and name handling ----------

    /// One stop per rank, so the returned order *is* the ranking.
    fn ranked() -> TestGtfs {
        let mut g =
            TestGtfs::new()
                .route("7", "7", 3, "x")
                .always("A")
                .trip("t", "7", "A", "St-Laurent");
        for (id, code, name) in [
            ("exact", "3009", "ZZZ EXACT CODE"),      // rank 0
            ("prefix", "3009111", "ZZZ CODE PREFIX"), // rank 1
            ("starts", "0001", "3009 STARTS NAME"),   // rank 2
            ("mid", "0002", "SOMEWHERE 3009 INSIDE"), // rank 3
        ] {
            g = g.stop(id, code, name).stop_time("t", id, 1, "10:00:00");
        }
        g
    }

    #[test]
    fn results_come_back_best_match_first() {
        let g = ranked();
        let names: Vec<String> = search_stops(g.conn(), "3009", &svc(&["A"]), 10)
            .unwrap()
            .into_iter()
            .map(|h| h.name)
            .collect();
        assert_eq!(
            names,
            vec![
                "ZZZ EXACT CODE",
                "ZZZ CODE PREFIX",
                "3009 STARTS NAME",
                "SOMEWHERE 3009 INSIDE",
            ],
            "an exact pole number must beat a code prefix, which beats a name \
             prefix, which beats a mid-name match"
        );
    }

    #[test]
    fn the_limit_keeps_the_best_matches_not_an_arbitrary_slice() {
        // This is the bug this ranking exists to prevent: filtering first and
        // ranking afterwards let an unordered cap discard the best matches.
        // Against the real feed, "st" matched 1207 stops and a 400-row cap
        // dropped 195 of the 283 whose name actually started with "st".
        let g = ranked();
        let top = search_stops(g.conn(), "3009", &svc(&["A"]), 1).unwrap();
        assert_eq!(top.len(), 1);
        assert_eq!(
            top[0].name, "ZZZ EXACT CODE",
            "a limit of 1 must keep rank 0"
        );
    }

    #[test]
    fn a_name_prefix_outranks_a_mid_name_match() {
        let g = TestGtfs::new()
            .route("7", "7", 3, "x")
            .always("A")
            .trip("t", "7", "A", "h")
            .stop("a", "0001", "ELGIN / RIDEAU")
            .stop("b", "0002", "RIDEAU / AUGUSTA")
            .stop_time("t", "a", 1, "10:00:00")
            .stop_time("t", "b", 2, "10:05:00");
        let names: Vec<String> = search_stops(g.conn(), "rideau", &svc(&["A"]), 10)
            .unwrap()
            .into_iter()
            .map(|h| h.name)
            .collect();
        assert_eq!(names, vec!["RIDEAU / AUGUSTA", "ELGIN / RIDEAU"]);
    }

    #[test]
    fn like_wildcards_are_matched_literally() {
        // Unescaped, "%" matches every stop and "_" matches any character, so
        // the search silently stops meaning what was typed.
        let g = ranked();
        assert!(
            search_stops(g.conn(), "%", &svc(&["A"]), 10)
                .unwrap()
                .is_empty()
        );
        assert!(
            search_stops(g.conn(), "_", &svc(&["A"]), 10)
                .unwrap()
                .is_empty()
        );
        // A single character is still a normal search.
        assert!(
            !search_stops(g.conn(), "3", &svc(&["A"]), 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_query_matching_nothing_returns_nothing() {
        let g = ranked();
        assert!(
            search_stops(g.conn(), "zzzznope", &svc(&["A"]), 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn the_platform_is_removed_from_the_name_only_when_it_is_really_there() {
        assert_eq!(strip_platform("RIDEAU A", "A"), "RIDEAU");
        assert_eq!(strip_platform("LINCOLN FIELDS 3A", "3A"), "LINCOLN FIELDS");
        // O-Train platforms are coded 1/2 but named by direction.
        assert_eq!(strip_platform("RIDEAU O-TRAIN EAST / EST", "2"), "RIDEAU");
        // No platform: the name is the information.
        assert_eq!(strip_platform("WESTBORO OFF ONLY", ""), "WESTBORO OFF ONLY");
        // Never strip a name down to nothing.
        assert_eq!(strip_platform("A", "A"), "A");
    }

    #[test]
    fn the_platform_is_only_stripped_at_a_word_boundary() {
        // Platform codes run A-H, so a name ending in that letter is plausible.
        // Without a boundary check "GREENBORO AVENUE" with platform "E" would
        // render "GREENBORO AVENU".
        assert_eq!(strip_platform("GREENBORO AVENUE", "E"), "GREENBORO AVENUE");
        assert_eq!(
            strip_platform("GREENBORO AVENUE E", "E"),
            "GREENBORO AVENUE"
        );
    }

    #[test]
    fn a_name_that_merely_ends_in_the_platform_letter_is_left_alone() {
        // "VANTAGE / AD. 303" looks like a platform suffix and is not one;
        // stripping is driven by platform_code, never by the name's shape.
        assert_eq!(strip_platform("VANTAGE / AD. 303", ""), "VANTAGE / AD. 303");
    }
}
