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
use std::sync::mpsc;
use std::time::{Duration, Instant};

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

        let child = pair
            .slave
            .spawn_command(CommandBuilder::new(env!("CARGO_BIN_EXE_otransit")))
            .expect("spawn otransit");
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
    s.send("q", 400);
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
    let mut s = Session::start();
    s.pump(1500);
    s.send("q", 400);
    assert_eq!(s.wait(5000), 0);
}

#[test]
fn refuses_to_run_without_a_terminal() {
    // Piped stdin means the cursor-position query can never be answered, so we
    // must say so plainly instead of hanging or dying on a cryptic timeout.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_otransit"))
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run otransit");
    let msg = String::from_utf8_lossy(&out.stderr);
    assert!(
        msg.contains("interactive terminal"),
        "expected a clear error, got: {msg}"
    );
    assert!(!out.status.success(), "should exit non-zero");
}
