mod app;
mod db;
mod dev;
mod fetch;
mod gtfs;
mod logo;
mod rt;
#[cfg(test)]
mod testing;
mod ui;

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
                _ => logo::print(w),
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
    let freshness = spawn_freshness_check(app.feed_etag());

    // Banner goes to stdout before the viewport exists, so it lands in
    // scrollback instead of being repainted every frame.
    logo::print(crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80));

    // Inline viewport, not the alternate screen: we claim a fixed block at the
    // bottom of the terminal and leave everything above it alone.
    enable_raw_mode()?;
    let mut terminal = Terminal::with_options(
        CrosstermBackend::new(std::io::stdout()),
        TerminalOptions {
            viewport: Viewport::Inline(ui::VIEWPORT_H),
        },
    )?;

    let res = run(&mut terminal, &mut app);

    disable_raw_mode()?;
    terminal.clear()?;
    terminal.show_cursor()?;

    // Only when there is something to do about it. Silence covers both "your
    // copy is current" and "we could not reach the server", and the second is
    // not worth a warning: you cannot update what you cannot download.
    if let Some(note) = freshness
        .recv_timeout(FRESHNESS_GRACE)
        .ok()
        .and_then(fetch::notice)
    {
        eprintln!("note: {note}");
    }
    res
}

/// How long the exit waits for the feed check, if it has not landed already.
///
/// The check measures 50-140ms. Without a grace period, quitting straight
/// after a glance -- which is what this app is for -- read an empty channel and
/// dropped the answer, so the notice appeared only for someone who lingered.
/// Long enough to cover that, short enough to be imperceptible on exit.
const FRESHNESS_GRACE: std::time::Duration = std::time::Duration::from_millis(300);

/// Ask, in the background, whether the published feed still matches our copy.
///
/// This replaced a warning computed from how many days ago the cache was
/// built. That number answered the wrong question: it moved every morning
/// whether or not OC Transpo published anything, so a cache `update` had just
/// confirmed was current still got called stale. The etag answers the question
/// actually being asked, and costs one round trip with no body.
fn spawn_freshness_check(etag: Option<String>) -> std::sync::mpsc::Receiver<fetch::Freshness> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(fetch::freshness(fetch::FEED_URL, etag.as_deref()));
    });
    rx
}

type Term = Terminal<CrosstermBackend<std::io::Stdout>>;

fn run(terminal: &mut Term, app: &mut App) -> Result<()> {
    while !app.quit {
        // The clock moves and the background fetch lands: both show up here,
        // once a frame, rather than only when a board is opened.
        app.tick();
        app.refresh_board()?;
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
