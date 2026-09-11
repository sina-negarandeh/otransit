"""Behavioural mutations, one per plausible wrong decision a second
implementation could make.

Each is (category, name, file, find, replace). `find` must appear exactly once,
and the result must compile: a mutation that does not build measures nothing.

These are Rust-shaped, so a Go port cannot reuse them literally. The categories
and the method transfer, and so does `run.py`, which scores by comparing
artifacts rather than by reading source.

Each is (category, name, file, find, replace). `find` must appear exactly once.
"""
M = [
# ---- time calculations ----
("time", "lateness truncates instead of rounding", "src/app/clock.rs",
 "    (d as f32 / 60.0).round() as i32\n}\n\n/// Format seconds", "    (d / 60) as i32\n}\n\n/// Format seconds"),
("time", "lateness does not wrap past midnight", "src/app/clock.rs",
 "    if d > 43_200 {\n        d -= 86_400;\n    }\n    if d < -43_200 {\n        d += 86_400;\n    }", ""),
("time", "mins_until does not wrap past midnight", "src/app/clock.rs",
 "    if d < -3600 {\n        d += 86_400;\n    }", ""),
("time", "the wait column drops its zero padding", "src/app/clock.rs",
 'format!("{}h {:02} min", m / 60, m % 60)', 'format!("{}h {} min", m / 60, m % 60)'),
("time", "the service day starts at plain midnight", "src/app/clock.rs",
 "    noon - Duration::hours(12)\n}",
 "    let _ = noon;\n    tz.from_local_datetime(&date.and_hms_opt(0, 0, 0).expect(\"midnight\"))\n        .single()\n        .expect(\"midnight\")\n}"),
("time", "mins_until truncates instead of rounding", "src/app/clock.rs",
 "    (d as f32 / 60.0).round() as i32\n}\n\n/// Width of the wait column", "    (d / 60) as i32\n}\n\n/// Width of the wait column"),
# ---- filtering ----
("filter", "the filter is case sensitive", "src/app/model.rs",
 "let hit = |s: &str| s.to_lowercase().contains(needle);", "let hit = |s: &str| s.contains(needle);"),
("filter", "a route matches on its number only", "src/app/model.rs",
 "Row::Route(r) => hit(&r.short_name) || hit(&r.long_name),", "Row::Route(r) => hit(&r.short_name),"),
("filter", "a stop does not match on its code", "src/app/model.rs",
 "Row::Stop(s) => hit(&s.name) || hit(&s.code),", "Row::Stop(s) => hit(&s.name),"),
# ---- cancellation ----
("cancel", "a cancelled trip is drawn as scheduled", "src/app/mod.rs",
 "d.canceled = rt.is_canceled(&d.trip_id);\n                    d.live = rt", "d.canceled = false;\n                    d.live = rt"),
("cancel", "a cancelled pin keeps its countdown", "src/app/mod.rs",
 "                        d.canceled = rt.is_canceled(&d.trip_id);", "                        d.canceled = false;"),
# ---- platform parsing ----
("platform", "a platform is parsed out of the stop name", "src/db/search.rs",
 "    let mut out: Vec<StopHit> = Vec::new();\n    for (stop_id, code, name, platform) in ranked {",
 "    let mut out: Vec<StopHit> = Vec::new();\n    for (stop_id, code, name, platform) in ranked {\n        let platform: String = if platform.is_empty() {\n            name.rsplit(' ').next().filter(|t| t.chars().any(|c| c.is_ascii_digit())).unwrap_or(\"\").to_string()\n        } else { platform };"),
("platform", "the platform is never stripped from the name", "src/db/search.rs",
 "pub fn strip_platform(name: &str, platform: &str) -> String {\n    if platform.is_empty() {",
 "pub fn strip_platform(name: &str, platform: &str) -> String {\n    if true || platform.is_empty() {"),
# ---- feed failure ----
("feed", "a refusal wipes the board it should keep", "src/app/poll.rs",
 "    !matches!(current, RtState::Ready(_))\n}", "    let _ = current;\n    true\n}"),
("feed", "a refusal is silently ignored", "src/app/poll.rs",
 "        Err(e) => with_state(slot, |s| {\n            if replaces_on_failure(s) {\n                *s = RtState::Failed(e.to_string());\n            }\n        }),",
 "        Err(e) => {\n            let _ = e;\n        }"),
# ---- retry / backoff ----
("backoff", "the backoff is a constant interval", "src/app/poll.rs",
 "        let next = (backoff * 2).clamp(MIN_BACKOFF_SECS, MAX_BACKOFF_SECS);\n        (next, next)",
 "        let _ = backoff;\n        (MIN_BACKOFF_SECS, MIN_BACKOFF_SECS)"),
("backoff", "success does not clear the backoff", "src/app/poll.rs",
 "        (rt::TTL_SECS as u64, 0)", "        (rt::TTL_SECS as u64, backoff)"),
("backoff", "an attempt is recorded where it was noticed", "src/replay/wire.rs",
 "            let at = self.schedule.due_at();", "            let at = app.epoch();"),
("backoff", "one answer is served per step", "src/replay/wire.rs",
 "        while self.schedule.due(app.epoch()) {",
 "        let mut once = true;\n        while once && self.schedule.due(app.epoch()) {\n            once = false;"),
# ---- rendering ----
("render", "truncation leaves no ellipsis", "src/ui/layout.rs",
 '_ => s.chars().take(max - 1).collect::<String>() + "…",', '_ => s.chars().take(max).collect::<String>(),'),
("render", "the weather label is cut instead of dropped", "src/ui/mod.rs",
 "    let note = note.filter(|n| n.chars().count() + 2 <= width);",
 "    let cut: Option<String> = note.map(|n| n.chars().take(width.saturating_sub(2)).collect());\n    let note = cut.as_deref();"),
("render", "the detour is drawn on every screen", "src/app/mod.rs",
 "        self.screen\n            .below_a_route()\n            .then(|| self.feeds.alert(&self.screen.route()?.short_name))\n            .flatten()",
 "        self.feeds.alert(&self.screen.route()?.short_name)"),
("render", "the cursor marker is not reserved on unselected rows", "src/ui/layout.rs",
 'pub(super) const MARKER_W: usize = 3;', 'pub(super) const MARKER_W: usize = 1;'),
# ---- pin identity ----
("pin", "a pin forgets the route it was made from", "src/app/model.rs",
 "                Some(&route.short_name),\n                Some(headsign.as_str()),",
 "                { let _ = route; None },\n                Some(headsign.as_str()),"),
("pin", "a pin forgets its direction", "src/app/model.rs",
 "                Some(&route.short_name),\n                Some(headsign.as_str()),",
 "                Some(&route.short_name),\n                { let _ = headsign; None },"),
# ---- navigation ----
# Commented out rather than written as `x = x.take()`, which has the same effect
# and reads as a no-op. The README tells a reader to check survivors for exactly
# that, so a mutation should not look like one.
("nav", "esc from a pin unwinds a path nobody walked", "src/app/mod.rs",
 "        self.returning_to = None;", "        // self.returning_to = None;"),
("nav", "the board is not re-queried as departures go", "src/app/mod.rs",
 "        if !gone {\n            return Ok(());\n        }", "        if true || !gone {\n            return Ok(());\n        }"),
# ---- midnight ----
("midnight", "yesterday's service is not consulted", "src/app/mod.rs",
 "        let yesterday = db::active_services(&conn, yday_date)?;", "        let yesterday = Vec::new();"),
("midnight", "yesterday's window is dropped from the query", "src/app/mod.rs",
 "            yesterday: &self.yesterday,", "            yesterday: &[],"),
("midnight", "an after-midnight trip is not marked", "src/db/browse.rs",
 "                after_midnight: shift > 0,", "                after_midnight: false,"),
# ---- unknown feed values ----
("unknown", "an unknown icon code draws a stand-in", "src/weather.rs",
 "        _ => return None,\n    })", "        _ => Sky::Cloud,\n    })"),
("unknown", "the night icon codes are not folded", "src/weather.rs",
 "    let day = if (30..=39).contains(&icon) {\n        icon - 30\n    } else {\n        icon\n    };", "    let day = icon;"),
("unknown", "every route gets every detour", "src/feeds.rs",
 "    pub fn alert(&self, short_name: &str) -> Option<String> {", "    pub fn alert(&self, unused: &str) -> Option<String> {\n        let _ = unused;\n        let short_name = \"44\";"),
]
