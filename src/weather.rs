//! Current conditions at the stop, from Environment and Climate Change Canada.
//!
//! The app already knows how long you will stand outside. This is the other
//! half of whether that is fine. One glyph, one condition, one temperature —
//! not a forecast, because a forecast is a different app.
//!
//! The source is MSC GeoMet's OGC API, which needs no key and is published
//! under the same Open Government Licence family as the transit feed. ECCC
//! labels the collection *experimental*, so it belongs in the same category as
//! the realtime endpoint with `beta` in its URL: it will change shape, and a
//! change reads as "no weather" rather than as an error.
//!
//! Deliberately not read: `windChill` and `humidex`. The live payload for
//! Ottawa carried `windChill: -2` at 20.7 °C in light rain, flagged
//! `qaValue: 100`. Both fields are there, both are quality-stamped, and one of
//! them was impossible. A "feels like" drawn from that is a confident lie, and
//! this app would rather say less.

use anyhow::{Context, Result};

/// Ottawa (Kanata – Orléans), the forecast area covering the transit network.
///
/// There is a second Ottawa area, `on-52` (Richmond – Metcalfe), and the
/// observing station for both is the airport. "Ottawa" is not one place to
/// this feed, so the choice is recorded here rather than implied.
pub const FEED_URL: &str =
    "https://api.weather.gc.ca/collections/citypageweather-realtime/items/on-118?f=json";

/// What the rule shows.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Weather {
    /// A single-cell glyph, or none for a code this does not know. Never two
    /// cells: the rule is measured in `chars`, and a double-width glyph would
    /// push the right edge off by one — the byte-versus-cell defect this
    /// project already has an entry for.
    pub glyph: Option<char>,
    /// The condition as published, lowercased.
    pub condition: String,
    /// Degrees Celsius, rounded.
    pub temp: i32,
}

impl Weather {
    /// One line, for the right-hand end of the rule.
    ///
    /// `·` separates the two facts, because that is what the status bar on the
    /// opposite rule uses between every one of its own. An em dash would read
    /// well and is already taken: it is what a board prints where a cancelled
    /// bus's countdown would go.
    pub fn label(&self) -> String {
        match self.glyph {
            Some(g) => format!("{g} {} · {}°", self.condition, self.temp),
            None => format!("{} · {}°", self.condition, self.temp),
        }
    }
}

/// Read the current conditions out of one citypage feature.
///
/// `None` for anything this cannot read, which includes a payload that changed
/// shape. The rule then draws as it always did.
pub fn parse(json: &str) -> Option<Weather> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let cc = v.pointer("/properties/currentConditions")?;

    // Both from the machine fields. `condition` is prose and `iconCode` is a
    // number, and the glyph comes from the number for the same reason a
    // detour's routes come from its tag: one of them is a contract.
    let condition = cc.pointer("/condition/en")?.as_str()?.trim();
    if condition.is_empty() {
        return None;
    }
    let celsius = cc.pointer("/temperature/value/en")?.as_f64()?;
    // Ottawa's record range is -39 to 38, so the rounded value is nowhere near
    // an i32 edge. Rejected rather than saturated if the feed ever says
    // otherwise, because a wrong temperature is worse than none.
    if !(-100.0..=100.0).contains(&celsius) {
        return None;
    }
    let icon = cc
        .pointer("/iconCode/value")
        .and_then(serde_json::Value::as_i64);

    Some(Weather {
        glyph: icon.and_then(sky).map(Sky::glyph),
        condition: condition.to_lowercase(),
        temp: celsius.round() as i32,
    })
}

/// The weather in one character.
///
/// The codes were read off the live feed rather than off a legend: a sweep of
/// every reporting station found 17 in use, each confirmed by the condition
/// text beside it. That sweep is also where the night rule came from — 0/30
/// Sunny/Clear, 2/32, 3/33 and 6/36 all pair, so 30..=39 are the night forms
/// of 0..=9, while 40 and up stand alone.
///
/// What the sky is doing, as far as this app draws it.
///
/// Named rather than mapped straight to a character, so the glyph and the word
/// for it are two exhaustive matches on the same type. A sixth kind then fails
/// to compile in both places, instead of falling through a catch-all to a
/// label reading "weather: weather".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Sky {
    Clear,
    Cloud,
    Rain,
    Snow,
    Haze,
}

impl Sky {
    /// Every glyph here occupies one cell. `⛅` and `⚡` read better and are
    /// both double-width, so neither is used.
    fn glyph(self) -> char {
        match self {
            Sky::Clear => '☀',
            Sky::Cloud => '☁',
            Sky::Rain => '⛆',
            Sky::Snow => '❄',
            Sky::Haze => '≈',
        }
    }
}

/// A code this does not know draws nothing rather than a placeholder. The
/// condition text still says what the weather is, so the cost is a picture and
/// not a fact — and a stand-in dot would collide with the `·` that separates
/// the two halves of the label.
fn sky(icon: i64) -> Option<Sky> {
    let day = if (30..=39).contains(&icon) {
        icon - 30
    } else {
        icon
    };
    Some(match day {
        0 | 1 => Sky::Clear,
        2 | 3 | 4 | 5 | 10 => Sky::Cloud,
        6 | 7 | 9 | 11..=15 | 19 | 27 | 28 | 45 | 46 | 47 => Sky::Rain,
        8 | 16 | 17 | 18 | 25 | 26 | 40 => Sky::Snow,
        20 | 23 | 24 | 44 => Sky::Haze,
        _ => return None,
    })
}

/// Fetch and read. Failure is not worth surfacing: weather you could not
/// download is a rule that draws plain, which is what it did before.
pub fn fetch() -> Result<Weather> {
    let body = ureq::get(FEED_URL)
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .context("fetching current conditions")?
        .into_string()
        .context("reading current conditions")?;
    parse(&body).context("current conditions were not in the payload")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ottawa as the feed served it, trimmed to the fields this reads — plus
    /// the two it refuses to. Checked in so no test reaches the network.
    const OTTAWA: &str = include_str!("../tests/weather.json");

    #[test]
    fn the_live_payload_reads_as_one_line() {
        let w = parse(OTTAWA).expect("Ottawa did not parse");
        assert_eq!(w.label(), "⛆ light rain · 21°");
    }

    #[test]
    fn a_temperature_is_rounded_rather_than_truncated() {
        // The feed publishes one decimal. 20.7 is 21 outside, not 20.
        let at = |c: f64| {
            parse(&format!(
                r#"{{"properties":{{"currentConditions":{{"condition":{{"en":"Cloudy"}},
                   "iconCode":{{"value":10}},"temperature":{{"value":{{"en":{c}}}}}}}}}}}"#
            ))
            .map(|w| w.temp)
        };
        assert_eq!(at(20.7), Some(21));
        assert_eq!(at(20.4), Some(20));
        assert_eq!(at(-0.6), Some(-1));
    }

    #[test]
    fn wind_chill_is_never_read() {
        // The fixture carries `windChill: -2` at 20.7 °C, quality-flagged 100,
        // exactly as the live feed served it. Anything reading it would put a
        // freezing "feels like" on a muggy September evening.
        assert!(OTTAWA.contains("\"windChill\""), "fixture lost the trap");
        let w = parse(OTTAWA).unwrap();
        assert_eq!(w.temp, 21, "read the wind chill instead of the temperature");
        assert!(!w.label().contains("-2"), "wind chill reached the label");
    }

    #[test]
    fn the_glyph_comes_from_the_code_and_not_the_prose() {
        // The prose says snow and the code says light rain. The code is the
        // machine field, so the code wins -- and the text is still shown as
        // published, because rewriting someone else's forecast is not this
        // app's job.
        let w = parse(
            r#"{"properties":{"currentConditions":{"condition":{"en":"Heavy Snow"},
               "iconCode":{"value":12},"temperature":{"value":{"en":3.0}}}}}"#,
        )
        .unwrap();
        assert_eq!(w.glyph, Some('⛆'));
        assert_eq!(w.condition, "heavy snow");
    }

    #[test]
    fn a_night_code_draws_the_same_weather_as_its_day_form() {
        // 30..=39 are the night forms of 0..=9. Confirmed against the live
        // feed: 0/30, 2/32, 3/33 and 6/36 each pair on the same condition.
        for day in [0, 1, 2, 3, 6] {
            assert_eq!(sky(day), sky(day + 30), "code {day} lost its night");
        }
        // 44 is Smoke, not the night form of 14. Above 39 they stand alone.
        assert_eq!(sky(44), Some(Sky::Haze));
    }

    #[test]
    fn an_unknown_code_keeps_the_condition_and_drops_the_picture() {
        // A code this does not know costs a glyph, not a fact.
        let w = parse(
            r#"{"properties":{"currentConditions":{"condition":{"en":"Ice Crystals"},
               "iconCode":{"value":99},"temperature":{"value":{"en":-30.0}}}}}"#,
        )
        .unwrap();
        assert_eq!(
            w.glyph, None,
            "invented a picture for a code it cannot read"
        );
        assert_eq!(w.label(), "ice crystals · -30°");
    }

    #[test]
    fn every_glyph_is_one_cell_wide() {
        // The rule is right-aligned and measured in `chars`, so a double-width
        // glyph would push its end off by one. `⛅` and `⚡` are the tempting
        // ones and both are Wide.
        for code in (0..50).chain([99]) {
            let Some(g) = sky(code).map(Sky::glyph) else {
                continue;
            };
            assert_eq!(
                unicode_width::UnicodeWidthChar::width(g),
                Some(1),
                "code {code} draws {g:?} in more than one cell"
            );
        }
    }

    #[test]
    fn a_payload_that_changed_shape_is_no_weather_rather_than_a_wrong_one() {
        // ECCC calls this collection experimental. A shape change has to read
        // as "nothing to show", which is a rule that draws plain.
        assert!(parse("{}").is_none());
        assert!(parse("not json").is_none());
        assert!(parse(r#"{"properties":{"currentConditions":{}}}"#).is_none());
        // A temperature with no condition beside it is half an answer.
        assert!(
            parse(
                r#"{"properties":{"currentConditions":{"condition":{"en":"  "},
                   "temperature":{"value":{"en":4.0}}}}}"#
            )
            .is_none()
        );
    }
}
