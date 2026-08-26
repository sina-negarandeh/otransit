//! Downloading and unpacking the published GTFS feed.
//!
//! OC Transpo republishes the schedule daily and asks that it be re-downloaded
//! each day, because GTFS-Realtime trip_ids are only guaranteed to match that
//! day's export. We use a conditional GET so an unchanged feed costs one
//! round-trip instead of 114MB.

use anyhow::{Context, Result, bail};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

pub const FEED_URL: &str =
    "https://oct-gtfs-emasagcnfmcgeham.z01.azurefd.net/public-access/GTFSExport.zip";

pub struct Download {
    /// Server's ETag, to skip the transfer next time.
    pub etag: Option<String>,
    pub bytes: u64,
}

/// What a response means, decided before a single byte is written to disk.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The cached copy is still current.
    NotModified,
    /// A body follows and should be written.
    Body,
}

/// Classify a response status.
///
/// The subtlety this exists for: ureq only raises `Err` for status >= 400, so
/// **304 Not Modified arrives as a perfectly good `Ok` response with an empty
/// body**. Treating it as a download writes a zero-byte file, which then fails
/// much later with "invalid Zip archive: Could not find EOCD".
pub fn classify(status: u16) -> Outcome {
    match status {
        304 => Outcome::NotModified,
        _ => Outcome::Body,
    }
}

/// A feed body is never legitimately empty. Reject it here, where the message
/// can say so, rather than letting the zip reader fail confusingly later.
pub fn check_body_len(bytes: u64) -> Result<()> {
    if bytes == 0 {
        bail!("feed returned an empty body");
    }
    Ok(())
}

/// Whether the published feed still matches the copy we hold.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Freshness {
    /// The server holds the same bytes we do.
    Current,
    /// A newer export is published.
    Moved,
    /// The feed answered, with an error. The URL we hold is wrong or the
    /// service is broken, and either way it needs a person.
    Broken(u16),
    /// No answer at all: offline, DNS, timed out, or we hold no etag to ask
    /// about. Nothing to act on.
    Unknown,
}

/// What to tell the user about the feed, if anything.
///
/// Separate from the check so the mapping from answer to message is testable:
/// the answer being right does not help if the wiring prints it for the wrong
/// one, and that is the shape of the defect this whole change exists to fix.
pub fn notice(freshness: Freshness) -> Option<String> {
    match freshness {
        Freshness::Moved => {
            Some("a newer schedule is published. Run `otransit update`.".to_string())
        }
        // Offline is not worth a warning; a feed that answers with an error is.
        // FEED_URL is compiled in, so this is the app's problem, not the
        // network's, and silence would hide it forever.
        Freshness::Broken(code) => Some(format!(
            "the schedule feed returned HTTP {code}. It may have moved; \
             otransit needs a new URL."
        )),
        Freshness::Current | Freshness::Unknown => None,
    }
}

/// Compare what the server publishes against the etag we stored, by asking
/// rather than by guessing from a date.
///
/// The etag is computed from the file, so a match means identical bytes. A date
/// only records when we last did something, which is a different question and
/// the reason this exists.
///
/// `HEAD`, so a moved feed costs nothing either: the answer is in the headers
/// and the 109MB body is never requested.
///
/// With no stored etag there is no question to ask, so this returns without
/// touching the network. Caches built before etags were recorded take that
/// path, which is why it is asserted on rather than assumed.
pub fn freshness(url: &str, etag: Option<&str>) -> Freshness {
    let Some(tag) = etag else {
        return Freshness::Unknown;
    };
    // Bounded so the thread cannot outlive any use for its answer; the caller
    // waits only briefly for it and never blocks the browser on it.
    let resp = ureq::head(url)
        .timeout(std::time::Duration::from_secs(5))
        .set("If-None-Match", tag)
        .call();
    // Every decision is `judge`'s; this only reshapes what ureq returns. A
    // status of None means the request never got an answer at all.
    match resp {
        Ok(r) => judge(Some(r.status()), r.header("etag"), tag),
        Err(ureq::Error::Status(code, _)) => judge(Some(code), None, tag),
        Err(_) => judge(None, None, tag),
    }
}

/// What a conditional response means for the copy we hold.
///
/// Split from the request so the decision can be tested without a network: no
/// test may reach the live feed, and this is the part that can be wrong.
fn judge(status: Option<u16>, served: Option<&str>, held: &str) -> Freshness {
    // No status at all: offline, DNS, timeout. Nothing was learned.
    let Some(status) = status else {
        return Freshness::Unknown;
    };
    if classify(status) == Outcome::NotModified {
        return Freshness::Current;
    }
    // The feed answered and said no. `FEED_URL` is compiled in, so this is the
    // app's problem rather than the network's, and it must not be silenced
    // alongside being offline.
    if status >= 400 {
        return Freshness::Broken(status);
    }
    match served {
        Some(now) if now == held => Freshness::Current,
        Some(_) => Freshness::Moved,
        // A 200 with no etag tells us nothing about which bytes those are, and
        // guessing "moved" here would nag on every launch.
        None => Freshness::Unknown,
    }
}

/// Fetch the feed to `dest`. Returns None when the server says 304 Not Modified.
pub fn download(url: &str, dest: &Path, prev_etag: Option<&str>) -> Result<Option<Download>> {
    let mut req = ureq::get(url).timeout(std::time::Duration::from_secs(600));
    if let Some(tag) = prev_etag {
        req = req.set("If-None-Match", tag);
    }

    let resp = match req.call() {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            bail!("feed returned HTTP {code} {}", r.status_text())
        }
        Err(e) => return Err(e).context("fetching the GTFS feed"),
    };

    if classify(resp.status()) == Outcome::NotModified {
        return Ok(None);
    }

    let etag = resp.header("etag").map(str::to_string);
    let total: u64 = resp
        .header("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; 1 << 16];
    let mut done: u64 = 0;
    let mut last_pct = u64::MAX;

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        done += n as u64;
        if let Some(pct) = (done * 100).checked_div(total)
            && pct != last_pct
            && pct.is_multiple_of(10)
        {
            eprint!(
                "\r  downloading {pct}% ({:.0} MB)",
                done as f64 / 1_048_576.0
            );
            last_pct = pct;
        }
    }
    out.flush()?;
    drop(out);
    eprintln!(
        "\r  downloaded {:.0} MB          ",
        done as f64 / 1_048_576.0
    );

    if let Err(e) = check_body_len(done) {
        let _ = fs::remove_file(dest);
        return Err(e);
    }
    Ok(Some(Download { etag, bytes: done }))
}

/// Unpack the archive into `dir`, replacing anything already there.
pub fn extract(zip_path: &Path, dir: &Path) -> Result<()> {
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir)?;

    let file = File::open(zip_path)?;
    let mut zip = zip::ZipArchive::new(file).context("reading the GTFS zip")?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        // enclosed_name() rejects paths that would escape the directory.
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let Some(file_name) = name.file_name() else {
            continue;
        };
        if entry.is_dir() {
            continue;
        }
        let out_path = dir.join(file_name);
        let mut out =
            File::create(&out_path).with_context(|| format!("writing {}", out_path.display()))?;
        std::io::copy(&mut entry, &mut out)?;
    }

    if !dir.join("stop_times.txt").exists() {
        bail!("archive did not contain stop_times.txt. Is this a GTFS feed?");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_304_means_the_copy_we_hold_is_current() {
        // The whole point: the server says "still that version" and sends no
        // body, so there is nothing to download and nothing to warn about.
        assert_eq!(judge(Some(304), None, "0xABC"), Freshness::Current);
    }

    #[test]
    fn a_different_etag_means_a_newer_export_is_published() {
        assert_eq!(judge(Some(200), Some("0xDEF"), "0xABC"), Freshness::Moved);
    }

    #[test]
    fn the_same_etag_on_a_200_still_means_current() {
        // Some caches answer 200 with the same etag rather than 304. That is
        // the same news, and treating it as "moved" would nag every launch.
        assert_eq!(judge(Some(200), Some("0xABC"), "0xABC"), Freshness::Current);
    }

    #[test]
    fn no_etag_at_all_is_unknown_rather_than_a_guess() {
        // Without one we cannot tell which bytes were served. Silence beats a
        // warning invented from a missing header.
        assert_eq!(judge(Some(200), None, "0xABC"), Freshness::Unknown);
    }

    #[test]
    fn a_feed_that_answers_with_an_error_is_not_the_same_as_being_offline() {
        // Offline is nothing to act on. A feed URL returning 404 is: it is
        // compiled into the binary, so only a person can fix it.
        assert_eq!(judge(Some(404), None, "0xABC"), Freshness::Broken(404));
        assert_eq!(judge(Some(503), None, "0xABC"), Freshness::Broken(503));
        assert_eq!(judge(None, None, "0xABC"), Freshness::Unknown);
    }

    #[test]
    fn a_cache_with_no_etag_asks_nothing_and_reports_unknown() {
        // No stored etag, no question to ask, so this returns before touching
        // the network -- which is why a test may call it at all: no test here
        // reaches the live feed. Caches built before etags were recorded take
        // this path on every launch.
        assert_eq!(
            freshness("http://0.0.0.0:1/never", None),
            Freshness::Unknown
        );
    }

    #[test]
    fn only_a_moved_or_broken_feed_is_worth_saying_anything_about() {
        // The wiring, not the decision: a correct answer printed for the wrong
        // case is the same defect in a different place.
        assert!(notice(Freshness::Moved).is_some(), "moved");
        assert!(notice(Freshness::Broken(404)).is_some(), "broken");
        assert_eq!(notice(Freshness::Current), None, "current");
        assert_eq!(notice(Freshness::Unknown), None, "offline");
    }

    #[test]
    fn a_broken_feed_names_the_status_so_it_can_be_diagnosed() {
        // "something went wrong" would send someone to the source; the code
        // says whether the URL is gone, forbidden, or the service is down.
        assert!(notice(Freshness::Broken(404)).unwrap().contains("404"));
    }

    #[test]
    fn a_304_is_not_a_download() {
        // This one shipped: ureq only errors for status >= 400, so a 304 came
        // back as Ok with an empty body, got written to disk, and failed later
        // as "invalid Zip archive: Could not find EOCD".
        assert_eq!(classify(304), Outcome::NotModified);
    }

    #[test]
    fn a_normal_response_carries_a_body() {
        assert_eq!(classify(200), Outcome::Body);
        assert_eq!(classify(206), Outcome::Body);
    }

    #[test]
    fn an_empty_body_is_rejected_where_the_message_can_explain_it() {
        let err = check_body_len(0).unwrap_err();
        assert!(err.to_string().contains("empty"), "got: {err}");
    }

    #[test]
    fn a_body_with_bytes_is_accepted() {
        assert!(check_body_len(1).is_ok());
        assert!(check_body_len(109 * 1024 * 1024).is_ok());
    }
}
