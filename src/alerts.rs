//! Service alerts: detours and route changes, from OC Transpo's updates feed.
//!
//! Not GTFS-Realtime. That specification has a ServiceAlerts feed and OC
//! Transpo's API does not serve one — `ServiceAlerts` and `VehiclePositions`
//! both answer 404, and only `TripUpdates` exists. The detours are published
//! instead as RSS on the developer page, and that is what this reads.
//!
//! It is the weakest of the three sources this app depends on. GTFS static and
//! GTFS-Realtime are specified formats; this is a CMS emitting RSS, where
//! `affectedRoutes-19, 42, 44, 48` is a convention rather than a contract. A
//! change in how they tag would look exactly like "no alerts", which is why
//! `probe` reports what parsed.

use anyhow::{Context, Result};

/// Where the alerts are published. Linked from the developer page, beside the
/// GTFS documentation, not from the realtime API.
///
/// The `/en/` is load-bearing: the developer page links the path without it,
/// which answers 301. That worked only because ureq follows redirects, and it
/// spent a round trip per launch to be told where the language prefix goes.
pub const FEED_URL: &str = "https://www.octranspo.com/en/feeds/updates-en/";

/// One published alert, reduced to what a terminal can act on.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Alert {
    /// The headline, shown as published. Route numbers are left in even though
    /// the screen showing it already knows the route: stripping them means
    /// rewriting someone else's prose, and the attempt mangled a third of the
    /// feed ("NCC Weekend Bikedays: to detour").
    pub title: String,
    /// Route short names the feed says are affected.
    pub routes: Vec<String>,
}

/// Alerts by route, ready to ask a question of.
#[derive(Default, Debug)]
pub struct Alerts(Vec<Alert>);

impl Alerts {
    /// What is published about this route, if anything.
    ///
    /// The first match wins. A route can appear in several alerts, and a screen
    /// has room for one; the feed lists newest first, which is the one worth
    /// showing.
    pub fn for_route(&self, short_name: &str) -> Option<&Alert> {
        self.0
            .iter()
            .find(|a| a.routes.iter().any(|r| r == short_name))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// Read the feed.
///
/// Only detours and route changes are kept. `General Message` is the third
/// kind and is almost entirely station relocations and park-and-ride notices —
/// real, but about places rather than routes, and there is nowhere in a
/// route-shaped screen to put them.
pub fn parse(xml: &str) -> Alerts {
    Alerts(
        split(xml, "item")
            .filter_map(|item| {
                let cats: Vec<String> = split(&item, "category").map(|c| clean(&c)).collect();
                if !cats
                    .iter()
                    .any(|c| c == "Detours" || c == "Route service change")
                {
                    return None;
                }
                let routes: Vec<String> = cats
                    .iter()
                    .filter_map(|c| c.strip_prefix("affectedRoutes-"))
                    .flat_map(|list| list.split(','))
                    .map(|r| r.trim().to_string())
                    .filter(|r| !r.is_empty())
                    .collect();
                // An alert naming no route cannot be shown on a route's screen.
                if routes.is_empty() {
                    return None;
                }
                let title = clean(&split(&item, "title").next()?);
                (!title.is_empty()).then_some(Alert { title, routes })
            })
            .collect(),
    )
}

/// The text inside every `<tag>…</tag>` at any depth, in document order.
///
/// A real XML parser would be a dependency for three tags in one document.
/// This does not need to handle namespaces, attributes with `>` in them, or
/// nesting of the same tag, and the feed has none of those.
fn split<'a>(xml: &'a str, tag: &'a str) -> impl Iterator<Item = String> + 'a {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut rest = xml;
    std::iter::from_fn(move || {
        let start = rest.find(&open)? + open.len();
        let end = rest[start..].find(&close)? + start;
        let found = rest[start..end].to_string();
        rest = &rest[end + close.len()..];
        Some(found)
    })
}

/// CDATA off, entities decoded, whitespace flattened.
fn clean(s: &str) -> String {
    let s = s.replace("<![CDATA[", "").replace("]]>", "");
    entities(&s)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Character references resolved, in one left-to-right pass.
///
/// A list of `replace` calls was the first shape and was wrong twice: the feed
/// is not limited to whichever references the list happens to name, and each
/// call sees the output of the last, so a literal `&amp;lt;` decoded twice
/// into `<`. Scanning once does neither.
fn entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        // `&` is one byte and so is `;`, so both slices land on boundaries.
        let found = rest
            .find(';')
            .and_then(|end| Some((end, reference(&rest[1..end])?)));
        match found {
            Some((end, c)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            // Not a reference. Emit the ampersand alone and resume after it,
            // so a real one later in the same run is still found rather than
            // swallowed by the search for a semicolon.
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// One reference body — what sits between the `&` and the `;`.
///
/// The five names are all XML defines; anything else has to be numeric, which
/// is how the feed writes its accents and emoji.
fn reference(body: &str) -> Option<char> {
    match body {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let digits = body.strip_prefix('#')?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

/// Fetch and parse. Failure is not an error worth surfacing: an alert you could
/// not download is one you cannot act on, and the browser stays quiet.
pub fn fetch() -> Result<Alerts> {
    let body = ureq::get(FEED_URL)
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .context("fetching the updates feed")?
        .into_string()
        .context("reading the updates feed")?;
    Ok(parse(&body))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three real entries, one of each kind the feed publishes, trimmed of
    /// their HTML bodies. Checked in so no test reaches the network.
    const FEED: &str = include_str!("../tests/updates.xml");

    #[test]
    fn a_route_finds_the_alert_that_names_it() {
        let a = parse(FEED);
        let pride = a.for_route("7").expect("route 7 is in the Pride detour");
        assert!(pride.title.contains("Pride"), "{}", pride.title);
        assert!(a.for_route("999").is_none(), "invented a route");
    }

    #[test]
    fn general_messages_are_left_out() {
        // The third kind is almost all station relocations: real, but about
        // places, and a route-shaped screen has nowhere to put them.
        let a = parse(FEED);
        assert!(
            !a.0.iter().any(|x| x.title.contains("Airport A relocation")),
            "a General Message was kept"
        );
    }

    #[test]
    fn a_route_service_change_is_kept_alongside_detours() {
        // Two kinds are shown, not one. Dropping this arm would silently keep
        // only detours.
        let a = parse(FEED);
        assert!(
            a.0.iter()
                .any(|x| x.title.contains("Permanent route change")),
            "route service changes were dropped: {:?}",
            a.0.iter().map(|x| &x.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn an_alert_naming_no_route_is_skipped() {
        // The screens that show these are all under one route. An alert with no
        // route has nowhere to appear, and keeping it would mean carrying a
        // second, unreachable kind.
        let xml = "<item><category>Detours</category><title>Something general</title></item>";
        assert_eq!(parse(xml).len(), 0);
    }

    #[test]
    fn the_routes_come_from_the_tag_not_from_the_prose() {
        // The title says "Routes 19, 42, 44, 48" and so does the category, but
        // only one of them is a contract. Reading the title would also match
        // the "2026" in a date and every stop code.
        let xml = "<item><category>Detours</category>\
                   <category>affectedRoutes-19, 42, 44, 48</category>\
                   <title>Detour near stop #1801 from 2026</title></item>";
        let a = parse(xml);
        assert!(a.for_route("44").is_some());
        assert!(a.for_route("1801").is_none(), "read a stop code as a route");
        assert!(a.for_route("2026").is_none(), "read a year as a route");
    }

    #[test]
    fn a_numeric_reference_in_a_headline_becomes_its_character() {
        // The feed does not restrict itself to the named five. The Pride
        // detour -- the newest one live as this was written, and the reason
        // this test exists -- ends in `&#127752;`, and a hand-written list of
        // entities rendered it as those nine literal characters on the screen.
        let xml = "<item><category>Detours</category>\
                   <category>affectedRoutes-7</category>\
                   <title>Detour: Capital Pride Festival &#127752; caf&#xe9;</title></item>";
        assert_eq!(
            parse(xml).for_route("7").unwrap().title,
            "Detour: Capital Pride Festival \u{1F308} café"
        );
    }

    #[test]
    fn an_ampersand_that_starts_nothing_survives_and_the_next_one_still_decodes() {
        // Decoding cannot be a scan for `;` from any `&`: a bare ampersand --
        // which CDATA permits and the feed's stop names carry -- would swallow
        // the text up to the next semicolon and take a real reference with it.
        let xml = "<item><category>Detours</category>\
                   <category>affectedRoutes-9</category>\
                   <title><![CDATA[Rideau & Sussex; see caf&#233;]]></title></item>";
        assert_eq!(
            parse(xml).for_route("9").unwrap().title,
            "Rideau & Sussex; see café"
        );
    }

    #[test]
    fn an_escaped_entity_is_decoded_once_and_not_twice() {
        // `&amp;lt;` is how the feed writes a literal "&lt;". Replacing
        // `&amp;` before `&lt;` turned it into "<" -- one pass left to right
        // is what stops the output of one substitution feeding the next.
        let xml = "<item><category>Detours</category>\
                   <category>affectedRoutes-8</category>\
                   <title>Wait &amp;lt; 5 min</title></item>";
        assert_eq!(parse(xml).for_route("8").unwrap().title, "Wait &lt; 5 min");
    }

    #[test]
    fn cdata_and_entities_come_out_as_text() {
        let xml = "<item><category><![CDATA[Detours]]></category>\
                   <category>affectedRoutes-5</category>\
                   <title><![CDATA[Bank &amp;  Slater   closed]]></title></item>";
        assert_eq!(
            parse(xml).for_route("5").unwrap().title,
            "Bank & Slater closed"
        );
    }

    #[test]
    fn a_feed_that_is_not_the_feed_yields_nothing_rather_than_failing() {
        // Their CMS changing shape must read as "no alerts", not as a crash --
        // but `probe` reports the count so the silence is visible somewhere.
        assert_eq!(parse("<html><body>404</body></html>").len(), 0);
        assert_eq!(parse("").len(), 0);
    }
}
