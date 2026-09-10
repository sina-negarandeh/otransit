mod alerts;
mod app;
mod db;
mod dev;
mod feeds;
mod fetch;
mod gtfs;
mod logo;
mod pins;
mod rt;
#[cfg(test)]
mod testing;
mod ui;
mod weather;

use anyhow::{Result, bail};
use app::App;
use chrono::Local;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::{Terminal, TerminalOptions, Viewport, backend::CrosstermBackend};
use rusqlite::Connection;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::Duration;

/// Where the cache lives.
///
/// `OTRANSIT_DB` overrides it. That exists so `tests/terminal.rs` can drive the
/// real binary against a cache it built itself: without it those tests read
/// whatever happens to be in the developer's cache directory, which is the
/// dependency on live data the project forbids, and they fail on any machine
/// that has never run `otransit update`.
fn db_path() -> PathBuf {
    if let Some(p) = std::env::var_os("OTRANSIT_DB") {
        return PathBuf::from(p);
    }
    directories::BaseDirs::new()
        .map(|b| b.cache_dir().join("otransit").join("gtfs.db"))
        .unwrap_or_else(|| PathBuf::from(".otransit.db"))
}

fn open_app(db: &PathBuf) -> Result<App> {
    if !db.exists() {
        bail!(
            "no schedule yet. Run:\n\n    otransit update\n\n\
             That downloads the published feed and builds the cache at {}.",
            db.display()
        );
    }
    let cache_dir = db
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    App::new(Connection::open(db)?, cache_dir)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |i: usize| args.get(i).map(String::as_str);
    let db = db_path();

    match arg(1) {
        Some("ingest") => {
            let dir = PathBuf::from(arg(2).unwrap_or("gtfs"));
            if !dir.join("stop_times.txt").exists() {
                bail!(
                    "no stop_times.txt in {}. Point me at an unzipped GTFS dir",
                    dir.display()
                );
            }
            eprintln!("ingesting {} -> {}", dir.display(), db.display());
            let t = std::time::Instant::now();
            gtfs::ingest(&dir, &db)?;
            eprintln!("done in {:.1}s", t.elapsed().as_secs_f32());
        }
        Some("--version" | "-V") => print!("{}", version()),
        Some("update") => update(&db)?,
        Some("probe") => dev::probe(&open_app(&db)?)?,
        Some("dump") => {
            let mut app = open_app(&db)?;
            dev::dump(&mut app, arg(2).unwrap_or("7"), arg(3))?;
        }
        Some("screenshot") => {
            let mut app = open_app(&db)?;
            let w = arg(2).and_then(|s| s.parse().ok()).unwrap_or(74);
            let h = arg(3).and_then(|s| s.parse().ok()).unwrap_or(18);
            let val = |k: &str| {
                args.iter()
                    .find_map(|a| a.strip_prefix(k).map(str::to_string))
            };
            let route = val("route=");
            let stop = val("stop=");
            let search = val("search=");
            dev::screenshot(
                &mut app,
                &dev::Shot {
                    w,
                    h,
                    train: args.iter().any(|a| a == "train"),
                    route: route.as_deref(),
                    stop: stop.as_deref(),
                    search: search.as_deref(),
                },
            )?;
        }
        Some("logo") => {
            let w = crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80);
            match arg(2) {
                Some("test") => logo::selftest(),
                _ => logo::print_alone(w),
            }
        }
        Some("-h" | "--help") => print!("{USAGE}"),
        _ => browse(&db)?,
    }
    Ok(())
}

const USAGE: &str = "\
otransit: browse OC Transpo schedules

    otransit                       drill down to departures
    otransit update                download today's feed and rebuild the cache
    otransit ingest <gtfs-dir>     build the cache from a local unzipped feed
    otransit probe                 report on the live realtime feed
    otransit dump <route> [stop]   headless walk of the query path
    otransit screenshot [w] [h]    render screens as text
                                   ('train', route=75, stop=WESTBORO, search=rideau)
    otransit logo [test]           startup mark; 'test' probes block rendering
    otransit --version             version and data attribution
";

/// Version, plus the attribution the Open Government Licence requires wherever
/// the data is used. The README alone does not cover a running binary.
fn version() -> String {
    format!(
        "otransit {}\n\
         \n\
         Contains information licensed under the Open Government Licence -\n\
         City of Ottawa. https://open.ottawa.ca/pages/open-data-licence\n\
         \n\
         Not affiliated with, endorsed by, or sponsored by OC Transpo or the\n\
         City of Ottawa.\n",
        env!("CARGO_PKG_VERSION")
    )
}

/// Download the published feed and rebuild the cache, atomically.
fn update(db: &PathBuf) -> Result<()> {
    let cache = db
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    std::fs::create_dir_all(&cache)?;

    // Reuse the previous ETag so an unchanged feed costs one round-trip — but
    // only if the cache was built by this version of the schema. Otherwise a
    // 304 would leave us on a layout the code no longer understands.
    let prev_etag = if db.exists() {
        let conn = Connection::open(db).ok();
        let current = conn
            .as_ref()
            .and_then(|c| gtfs::get_meta(c, "schema_version").ok().flatten())
            .unwrap_or_default();
        if current == gtfs::SCHEMA_VERSION {
            conn.and_then(|c| gtfs::stored_etag(&c))
        } else {
            eprintln!("  cache schema is out of date, rebuilding");
            None
        }
    } else {
        None
    };

    // Clear anything a previous failed run left behind.
    let zip = cache.join("GTFSExport.zip");
    let dir = cache.join("gtfs-extract");
    let _ = std::fs::remove_file(&zip);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(cache.join("gtfs.db.new"));

    eprintln!("checking {}", fetch::FEED_URL);
    let Some(dl) = fetch::download(fetch::FEED_URL, &zip, prev_etag.as_deref())? else {
        println!("schedule already current (304 Not Modified)");
        return Ok(());
    };

    eprintln!("  extracting...");
    fetch::extract(&zip, &dir)?;

    // Build alongside the live cache and swap at the end, so a failure part way
    // through can never leave you with no schedule at all.
    let tmp = cache.join("gtfs.db.new");
    let _ = std::fs::remove_file(&tmp);
    gtfs::ingest(&dir, &tmp)?;
    {
        let conn = Connection::open(&tmp)?;
        if let Some(tag) = &dl.etag {
            gtfs::set_meta(&conn, "etag", tag)?;
        }
        // Provenance, beside `ingested_from`: what this cache was built from
        // and when. Nothing branches on it, so a local date is what a person
        // reading it would want.
        gtfs::set_meta(&conn, "ingested_on", &Local::now().date_naive().to_string())?;
    }
    std::fs::rename(&tmp, db)?;

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&zip);
    println!("schedule updated ({:.0} MB)", dl.bytes as f64 / 1_048_576.0);
    Ok(())
}

fn browse(db: &PathBuf) -> Result<()> {
    // Before the cache: a pipe cannot be fixed by running `otransit update`,
    // and the inline viewport asks the terminal where the cursor is (ESC[6n)
    // and waits for the reply, so it needs a real tty on both ends.
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!(
            "otransit needs an interactive terminal.\n\
             For non-interactive use try:  otransit dump <route>"
        );
    }

    let mut app = open_app(db)?;
    if !app.has_service_today() {
        eprintln!("warning: no services active for today. Try: otransit update");
    }
    // Ask the server whether the feed has moved, off the main thread, so a
    // slow or absent network costs the browser nothing. The answer is read
    // after the viewport closes: it is advice for next time, not something to
    // act on mid-board, and it does not belong competing for the status bar.
    let check = FeedCheck::start(app.cache_etag());

    // Banner goes to stdout before the viewport exists, so it lands in
    // scrollback instead of being repainted every frame.
    logo::print(crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80));

    let res = in_viewport(&mut app);

    // After the terminal is its own again, so the note lands in scrollback
    // rather than under a viewport that is about to be cleared.
    if let Some(note) = check.note() {
        eprintln!("note: {note}");
    }
    res
}

/// Run the browser inside an inline viewport, restoring the terminal after.
///
/// Not the alternate screen: we claim a fixed block at the bottom and leave
/// everything above it alone. Paired here so the setup cannot outlive the
/// teardown, and so `browse` reads as the sequence it is.
fn in_viewport(app: &mut App) -> Result<()> {
    enable_raw_mode()?;
    let mut terminal = Terminal::with_options(
        CrosstermBackend::new(std::io::stdout()),
        TerminalOptions {
            viewport: Viewport::Inline(ui::VIEWPORT_H),
        },
    )?;

    let res = run(&mut terminal, app);

    disable_raw_mode()?;
    terminal.clear()?;
    terminal.show_cursor()?;
    res
}

/// A feed check running in the background, and what to say when it lands.
///
/// One type rather than a spawner, a constant and a mapping in three places:
/// starting the check, waiting for it and turning the answer into English are
/// the same feature, and a reader should find them together.
///
/// The message lives here rather than in `fetch` because it names the binary
/// and one of its subcommands. `fetch` speaks HTTP; what to tell someone about
/// the result is the CLI's business.
struct FeedCheck(std::sync::mpsc::Receiver<fetch::Freshness>);

impl FeedCheck {
    /// Ask whether the published feed still matches our copy, off the main
    /// thread, so a slow or absent network costs the browser nothing.
    ///
    /// This replaced a warning computed from how many days ago the cache was
    /// built. That number answered the wrong question: it moved every morning
    /// whether or not OC Transpo published anything, so a cache `update` had
    /// just confirmed was current still got called stale. The etag answers the
    /// question actually being asked, and costs one round trip with no body.
    fn start(etag: Option<String>) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(fetch::freshness(fetch::FEED_URL, etag.as_deref()));
        });
        Self(rx)
    }

    /// What to tell the user, waiting briefly if the answer has not arrived.
    ///
    /// A conditional HEAD returns in well under the time it takes to notice a
    /// pause, but it is not instant, and quitting straight after a glance is
    /// what this app is for. Reading without waiting at all found an empty
    /// channel and dropped the answer, so the notice reached only someone who
    /// lingered. The wait is bounded so a slow network cannot hold the exit.
    fn note(self) -> Option<String> {
        self.0.recv_timeout(Self::GRACE).ok().and_then(Self::say)
    }

    const GRACE: std::time::Duration = std::time::Duration::from_millis(300);

    /// Only when there is something to do about it. Silence covers both "your
    /// copy is current" and "we could not reach the server", and the second is
    /// not worth a warning: you cannot update what you cannot download.
    fn say(freshness: fetch::Freshness) -> Option<String> {
        match freshness {
            fetch::Freshness::Moved => {
                Some("a newer schedule is published. Run `otransit update`.".to_string())
            }
            // FEED_URL is compiled in, so a feed answering with an error is
            // this program's problem, and silence would hide it forever.
            fetch::Freshness::Broken(code) => Some(format!(
                "the schedule feed returned HTTP {code}. It may have moved; \
                 otransit needs a new URL."
            )),
            fetch::Freshness::Current | fetch::Freshness::Unknown => None,
        }
    }
}

type Term = Terminal<CrosstermBackend<std::io::Stdout>>;

fn run(terminal: &mut Term, app: &mut App) -> Result<()> {
    while !app.quit {
        // The clock moves and the background fetch lands: both show up here,
        // once a frame, rather than only when a board is opened.
        app.tick();
        app.refresh()?;
        app.apply_realtime();
        terminal.draw(|f| ui::draw(f, app))?;
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Press => handle_key(app, k)?,
            // Resize needs no handling here: Terminal::draw calls autoresize,
            // which repositions the inline viewport and clears it. Clearing
            // from here instead would erase from the *old* origin.
            _ => {}
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, k: KeyEvent) -> Result<()> {
    let typing = !app.screen.typed().is_empty();
    match k.code {
        KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => app.quit = true,
        KeyCode::Char('q') if !typing => app.quit = true,
        KeyCode::Enter => app.enter()?,
        KeyCode::Up => app.move_by(-1),
        KeyCode::Down => app.move_by(1),
        KeyCode::Char('k') if !typing => app.move_by(-1),
        KeyCode::Char('j') if !typing => app.move_by(1),
        // esc clears the filter first, then walks back up a level
        KeyCode::Esc => {
            if typing {
                app.clear_filter()?;
            } else {
                app.back()?;
            }
        }
        // "/" jumps to the stop search from anywhere, unless you're mid-word.
        KeyCode::Char('/') if !typing => app.focus_search()?,
        // Guarded on being a board, not on the filter being empty. `!typing`
        // is true exactly when you start typing, so it swallowed the first
        // keystroke on every list screen and no stop beginning with P could be
        // searched for.
        KeyCode::Char('p') if app.board_pin().is_some() => app.toggle_pin(),
        KeyCode::Backspace => {
            if !app.pop_filter()? {
                app.back()?;
            }
        }
        KeyCode::Char(c) => app.push_filter(c)?,
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key press, as the event loop delivers it.
    fn press(app: &mut App, c: char) {
        handle_key(app, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)).unwrap();
    }
    use fetch::Freshness;

    #[test]
    fn only_a_moved_or_broken_feed_is_worth_saying_anything_about() {
        // The wiring, not the decision: an answer that is right and printed for
        // the wrong case is the defect this whole change exists to fix, in a
        // different place.
        assert!(FeedCheck::say(Freshness::Moved).is_some(), "moved");
        assert!(FeedCheck::say(Freshness::Broken(404)).is_some(), "broken");
        assert_eq!(FeedCheck::say(Freshness::Current), None, "current");
        assert_eq!(FeedCheck::say(Freshness::Unknown), None, "offline");
    }

    #[test]
    fn a_broken_feed_names_the_status_so_it_can_be_diagnosed() {
        // "something went wrong" sends someone to the source; the code says
        // whether the URL is gone, forbidden, or the service is down.
        assert!(
            FeedCheck::say(Freshness::Broken(404))
                .unwrap()
                .contains("404")
        );
    }

    #[test]
    fn a_letter_that_does_nothing_here_still_reaches_the_filter() {
        // `p` was guarded on "not already typing", which is true exactly when
        // the filter is empty -- when you start typing. On a list screen it
        // matched, found no board to pin, and swallowed the keystroke, so no
        // stop beginning with P could be searched for.
        let g = crate::testing::TestGtfs::new()
            .route("5", "5", 3, "0057B8")
            .always("A")
            .trip("t5", "5", "A", "Elmvale")
            .stop("s1", "0001", "PIMISI")
            .stop_time("t5", "s1", 1, "10:00:00");
        let mut app = App::offline(
            g.into_conn(),
            chrono::NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
            9 * 3600,
            None,
        )
        .unwrap();
        app.enter().unwrap(); // modes -> routes
        app.enter().unwrap(); // routes -> directions
        app.enter().unwrap(); // directions -> stops

        press(&mut app, 'p');
        press(&mut app, 'i');
        assert_eq!(app.screen.typed(), "pi", "the p was eaten");
    }

    #[test]
    fn p_still_pins_from_a_board() {
        let dir = std::env::temp_dir().join("otransit-main-pin");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let g = crate::testing::TestGtfs::new()
            .route("5", "5", 3, "0057B8")
            .always("A")
            .trip("t5", "5", "A", "Elmvale")
            .stop("s1", "0001", "PIMISI")
            .stop_time("t5", "s1", 1, "10:00:00");
        let mut app = App::offline(
            g.into_conn(),
            chrono::NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
            9 * 3600,
            Some(dir.join("pins")),
        )
        .unwrap();
        for _ in 0..4 {
            app.enter().unwrap();
        }
        assert!(app.board_pin().is_some(), "not on a board");
        press(&mut app, 'p');
        assert_eq!(app.board_pin(), Some(pins::PinState::Pinned));
    }
}
