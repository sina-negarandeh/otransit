//! What the app emits to a real terminal.
//!
//! `cargo test`'s unit tests render through ratatui's `TestBackend`, which has
//! no terminal behind it — so nothing there can observe the escape sequences we
//! actually write. This drives the real binary through a pty instead.
//!
//! The binary path comes from `CARGO_BIN_EXE_otransit`, which Cargo builds and
//! injects for integration tests, so this can never test a stale binary.

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, mpsc};
use std::time::{Duration, Instant};

/// A cache built by this test, from a feed written by this test.
///
/// The binary refuses to start without one, so these tests used to depend on
/// whatever `otransit update` had left in the developer's cache directory:
/// live data, changing daily, absent on any machine that had never run it. They
/// passed here for months and failed the first time CI ran them.
///
/// One service, one route, one stop, departures across the day. Nothing here
/// asserts on the schedule; it exists so the app has something to draw.
fn cache() -> &'static Path {
    static CACHE: OnceLock<PathBuf> = OnceLock::new();
    CACHE.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("otransit-pty-{}", std::process::id()));
        let feed = dir.join("feed");
        std::fs::create_dir_all(&feed).expect("create feed dir");
        for (name, body) in [
            ("agency.txt", "agency_id,agency_name\n1,Test\n"),
            (
                "routes.txt",
                "route_id,route_short_name,route_long_name,route_type,route_color\n\
                 7,7,Blair <> Kanata,3,0057B8\n",
            ),
            (
                "stops.txt",
                "stop_id,stop_code,stop_name,stop_lat,stop_lon\n\
                 S1,3009,RIDEAU A,45.0,-75.0\n",
            ),
            (
                "trips.txt",
                "route_id,service_id,trip_id,trip_headsign,direction_id\n\
                 7,EVERY,t1,Blair,0\n7,EVERY,t2,Blair,0\n",
            ),
            (
                "calendar.txt",
                "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,\
                 start_date,end_date\nEVERY,1,1,1,1,1,1,1,20200101,20991231\n",
            ),
            ("calendar_dates.txt", "service_id,date,exception_type\n"),
            (
                "stop_times.txt",
                "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
                 t1,06:00:00,06:00:00,S1,1\nt2,23:30:00,23:30:00,S1,1\n",
            ),
        ] {
            std::fs::write(feed.join(name), body).expect("write feed file");
        }

        let db = dir.join("gtfs.db");
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_otransit"))
            .args(["ingest", feed.to_str().expect("utf-8 path")])
            .env("OTRANSIT_DB", &db)
            .output()
            .expect("run otransit ingest");
        assert!(
            out.status.success(),
            "ingest failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        db
    })
}

/// Switching to the alternate screen. Emitting this would wipe the user's
/// scrollback — the exact regression the inline design exists to prevent.
const SMCUP: &[u8] = b"\x1b[?1049h";

/// Device Status Report: "where is the cursor?". ratatui's inline viewport
/// blocks on the answer, and a bare pty has no terminal emulator to give one.
const DSR: &[u8] = b"\x1b[6n";

const ROWS: u16 = 30;
const COLS: u16 = 90;

/// A live session with the real binary attached to a pty.
struct Session {
    writer: Box<dyn Write + Send>,
    rx: mpsc::Receiver<Vec<u8>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    output: Vec<u8>,
}

impl Session {
    fn start() -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: ROWS,
                cols: COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open pty");

        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_otransit"));
        cmd.env("OTRANSIT_DB", cache());
        let child = pair.slave.spawn_command(cmd).expect("spawn otransit");
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().expect("reader");
        let writer = pair.master.take_writer().expect("writer");
        drop(pair.master);

        // Read on a thread: the pty has no EOF until the child exits, so a
        // blocking read in the test body could hang forever.
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });

        Self {
            writer,
            rx,
            child,
            output: Vec::new(),
        }
    }

    /// Collect output for `ms`, answering cursor-position queries as a real
    /// terminal would.
    fn pump(&mut self, ms: u64) {
        let deadline = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < deadline {
            let Ok(chunk) = self.rx.recv_timeout(Duration::from_millis(50)) else {
                continue;
            };
            let replies = chunk.windows(DSR.len()).filter(|w| *w == DSR).count();
            self.output.extend_from_slice(&chunk);
            for _ in 0..replies {
                let _ = write!(self.writer, "\x1b[{ROWS};1R");
                let _ = self.writer.flush();
            }
        }
    }

    fn send(&mut self, keys: &str, settle_ms: u64) {
        self.writer.write_all(keys.as_bytes()).expect("write keys");
        self.writer.flush().expect("flush");
        self.pump(settle_ms);
    }

    /// Whether the child has already gone, without waiting for it.
    ///
    /// `wait` consumes the session because it is the end of one. This asks the
    /// same question mid-session, for a test whose claim is that the browser is
    /// still running.
    fn exited(&mut self) -> Option<u32> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.exit_code()),
            _ => None,
        }
    }

    /// Wait for the child to exit, failing rather than hanging.
    fn wait(mut self, timeout_ms: u64) -> u32 {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                return status.exit_code();
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                panic!("otransit did not exit within {timeout_ms}ms");
            }
            self.pump(50);
        }
    }

    fn saw(&self, needle: &[u8]) -> usize {
        self.output
            .windows(needle.len())
            .filter(|w| *w == needle)
            .count()
    }
}

#[test]
fn never_takes_over_the_terminal() {
    // The whole inline design rests on this. No unit test can see it, because
    // TestBackend never writes an escape sequence anywhere.
    let mut s = Session::start();
    s.pump(1500);
    s.send("\r", 500); // Bus
    s.send("7", 500); // filter
    s.send("\r", 500); // route
    s.send("\r", 500); // direction
    s.send("\r", 700); // stop -> departures

    let alt = s.saw(SMCUP);
    let bytes = s.output.len();

    // Five escapes out: the board, the stops, the directions, the routes, and
    // the first screen, which is the floor `back` falls to. `q` is a letter
    // everywhere now, so it cannot end this.
    for _ in 0..5 {
        s.send("\x1b", 300);
    }
    let code = s.wait(5000);

    assert_eq!(alt, 0, "emitted {alt} alternate-screen sequences");
    assert!(
        bytes > 500,
        "only {bytes} bytes drawn; did it render at all?"
    );
    assert_eq!(code, 0, "exited with {code}");
}

#[test]
fn quits_cleanly_from_the_first_screen() {
    // `esc` is the exit, because the first screen is the floor `back` falls to.
    // `q` used to do it and no longer does at all: it was guarded on "nothing
    // typed yet", which is true at the start of every search, so it quit on the
    // first keystroke of a search for Queensway. 58 stops begin with Q.
    let mut s = Session::start();
    s.pump(1500);
    s.send("\x1b", 400);
    assert_eq!(s.wait(5000), 0);
}

#[test]
fn a_letter_does_not_quit_from_the_first_screen() {
    // The pty is the only place this can be proved end to end: `q` reaching the
    // filter instead of the exit is a claim about the real binary's key
    // handling, and no fixture presses `q` at all.
    let mut s = Session::start();
    s.pump(1500);
    s.send("q", 400);
    s.send("j", 200);
    s.send("k", 200);
    assert_eq!(s.exited(), None, "a letter quit the browser");

    // Two escapes, and the first one proves the letters landed: `esc` clears a
    // filter before it leaves a screen, so a session that had eaten the keys
    // would exit on the first press.
    s.send("\x1b", 400);
    assert_eq!(
        s.exited(),
        None,
        "the filter was empty, so the letters were eaten"
    );
    s.send("\x1b", 400);
    assert_eq!(s.wait(5000), 0);
}

#[test]
fn refuses_to_run_without_a_terminal() {
    // Piped stdin means the cursor-position query can never be answered, so we
    // must say so plainly instead of hanging or dying on a cryptic timeout.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_otransit"))
        .stdin(std::process::Stdio::null())
        .env("OTRANSIT_DB", cache())
        .output()
        .expect("run otransit");
    let msg = String::from_utf8_lossy(&out.stderr);
    assert!(
        msg.contains("interactive terminal"),
        "expected a clear error, got: {msg}"
    );
    assert!(!out.status.success(), "should exit non-zero");

    // The same, with no cache at all: a pipe cannot be fixed by running
    // `otransit update`, so the terminal check has to come first. It did not,
    // and on a machine with no cache this reported the wrong problem.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_otransit"))
        .stdin(std::process::Stdio::null())
        .env("OTRANSIT_DB", cache().with_file_name("absent.db"))
        .output()
        .expect("run otransit");
    let msg = String::from_utf8_lossy(&out.stderr);
    assert!(
        msg.contains("interactive terminal"),
        "with no cache, expected the terminal error first, got: {msg}"
    );
}
