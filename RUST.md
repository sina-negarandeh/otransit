# Rust standards

The conventions this project follows, and the commands that enforce them.
These are not aspirational: the codebase currently satisfies all of them, and
it should stay that way.

## Contents

- [The standards](#the-standards)
- [Gate before calling anything done](#gate-before-calling-anything-done)
- [Formatting](#formatting)
- [Linting](#linting)
- [API and library design](#api-and-library-design)
- [Language semantics and the standard library](#language-semantics-and-the-standard-library)
- [Edition](#edition)
- [Project decisions](#project-decisions)

## The standards

| Standard | What it governs | Enforced by |
|---|---|---|
| [Rust Style Guide](https://doc.rust-lang.org/style-guide/) | Formatting and layout | `cargo fmt` |
| [rustfmt](https://rust-lang.github.io/rustfmt/) | The above, automatically | `cargo fmt --check` |
| [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) | Idiomatic, ergonomic interfaces | review |
| [Clippy](https://rust-lang.github.io/rust-clippy/) | Idiom, correctness, complexity, performance | `cargo clippy` |
| [The Rust Reference](https://doc.rust-lang.org/reference/) | Language semantics; the authority on what code *means* | review |
| [Standard library docs](https://doc.rust-lang.org/std/) | Prefer established APIs over bespoke ones | review |
| [Edition Guide](https://doc.rust-lang.org/edition-guide/) | Edition-specific idiom | `edition` in Cargo.toml |

## Gate before calling anything done

All four, every time. Any non-zero output is a failure, not a note.

```bash
cargo fmt --check                      # no diffs
cargo clippy --release --all-targets   # zero warnings
cargo build --release                  # zero warnings
cargo test --release                   # all green
```

Current state: **0 diffs, 0 clippy warnings, 0 build warnings, 78 tests
passing.** There is no backlog to excuse a new one.

## Formatting

`cargo fmt` decides. Default `rustfmt` settings, no `rustfmt.toml`, no
hand-tuning — the value of the standard is that it is not negotiated per file.

Do not fight it. If a line reads badly after formatting, the fix is usually
shorter names or an extracted binding, not a manual override.

## Linting

Lints live in the **`[lints]` table in `Cargo.toml`**, not in crate-level
attributes — one place, applied to every target, visible to anyone reading the
manifest.

**Default clippy must stay at zero.** It catches real defects, not style
preferences — `manual_is_multiple_of`, `needless_range_loop`, and
`manual_checked_division` all fired here and all pointed at genuinely better
code.

**`clippy::pedantic` is not enabled wholesale.** It reports ~93 findings, and
the bulk are `cast_possible_truncation` on deliberate `as u16` casts in layout
code where the value is bounded by a terminal dimension. Silencing ~90 of those
with `#[allow]` would train the eye to skip lint output — the opposite of the
point.

Instead, ten pedantic lints that earn their keep are promoted to warnings in
the manifest: `uninlined_format_args`, `redundant_closure_for_method_calls`,
`needless_pass_by_value`, `unnested_or_patterns`, `match_same_arms`,
`needless_raw_string_hashes`, `explicit_iter_loop`, `implicit_clone`,
`inefficient_to_string`, `manual_let_else`.

**`unsafe_code = "forbid"`** is set under `[lints.rust]`. There is no `unsafe`
in this codebase and no reason for any, and the compiler now enforces that
rather than a document asserting it.

Still worth running occasionally to see what full pedantic would say:

```bash
cargo clippy --release --all-targets -- -W clippy::pedantic
```

When a lint is genuinely wrong, `#[allow]` it **narrowly** — on the item, never
the module or crate — with a comment saying why. The one instance:
`poll_realtime` takes owned arguments because it is moved onto a detached
thread, so `needless_pass_by_value` cannot be satisfied.

## API and library design

From the API Guidelines, the points that bite in a codebase this size:

- **Naming follows conventions.** `as_` borrows, `to_` clones/converts, `into_`
  consumes. `TestGtfs::into_conn` consumes; `TestGtfs::conn` borrows.
- **Errors carry context.** `anyhow::Context` at every boundary where the
  message would otherwise be a bare `NotFound`. An error a user sees should name
  the file, the column, or the URL.
- **Document what isn't obvious from the signature**, not what is. `/// Returns
  the name` on `fn name()` is noise; a note that `arr` may exceed 86400 is not.
- **Take the least specific argument that works** — `&str` over `&String`,
  `&[T]` over `&Vec<T>`, `&Path` over `&PathBuf`.
- **Keep visibility tight.** `pub` only where another module genuinely needs it;
  `pub(crate)` or private otherwise. `gtfs::create_schema` is `#[cfg(test)]`
  because only fixtures call it.

## Language semantics and the standard library

- **Reach for `std` first.** `is_multiple_of`, `repeat_n`, `rem_euclid`,
  `slice::fill`, `IsTerminal` all replaced hand-rolled versions here. If a
  helper feels like it should exist, check that it doesn't.
- **No `unsafe`.** Forbidden in the manifest, so this is a compiler error rather
  than a convention.
- **No `unwrap`/`expect` on a path a user can reach.** Test code and genuine
  invariants are fine, and an `expect` should say what was violated.
- **Integer casts are a decision, not a convenience.** Prefer `i64::from(x)` or
  `try_into()` where the range isn't obviously bounded. A bare `as` needs the
  bound to be evident from nearby code.

## Edition

**2024**, per `Cargo.toml`, with `rust-version = "1.87"` recording the floor
(edition 2024 needs 1.85; `is_multiple_of` on unsigned integers needs 1.87).
The MSRV is derived from what the code uses, not verified against an older
toolchain — treat it as a claim to check before anyone depends on it.

Migrating from 2021 surfaced one genuine semantic change, worth knowing because
the same shape will recur:

`if let Ok(guard) = mutex.lock()` holds the guard for the **whole** `if let`,
including its `else`, and 2024 changes when that temporary drops relative to
other locals. The fix was not to silence it but to give the lock its own scope
(`app::with_state`), which is better code under any edition. If you see
`tail-expr-drop-order`, restructure rather than suppress.

## Project decisions

Recorded so they don't get re-litigated:

- **Unit tests stay in the source file; `tests/` is for integration only.**
  This is Rust's convention, and here it is also forced: `otransit` is a binary
  crate with no `src/lib.rs`, so a file in `tests/` compiles as a separate crate
  with nothing to link against — it cannot see our modules at all. That is why
  `tests/terminal.rs` spawns the binary rather than calling functions.

  Splitting into `lib.rs` + a thin `main.rs` would let `tests/` import, but it
  would force `pub` onto internals that are deliberately private
  (`match_rank`, `with_state`, `placeholders`) purely to test them, and nothing
  else consumes this code. Not worth it unless a second consumer appears.
- **No `rustfmt.toml`.** Defaults only.
- **Lints belong in `Cargo.toml`**, not in crate attributes.
- **Full pedantic is advisory; a curated ten are gating.** See above.
- **`unsafe_code` is forbidden**, not merely discouraged.
- **`#[cfg(test)]` on test-only public items**, so they don't widen the release
  API or trip `dead_code`.
- **Test modules go last in a file.** Clippy's `items_after_test_module` fires
  otherwise, and it has caught real ordering mistakes here twice.
- **Dev-dependencies stay out of the release build.** `tempfile` is under
  `[dev-dependencies]` for exactly this reason.
