# Rust standards

The conventions this project follows, and the commands that enforce them. These
are not aspirational. The codebase satisfies all of them today, and it must
stay that way.

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
| [The Rust Reference](https://doc.rust-lang.org/reference/) | Language semantics. The authority on what code *means* | review |
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

Current state: **0 diffs, 0 clippy warnings, 0 build warnings, 177 tests
passing.** There is no backlog to excuse a new failure.

## Formatting

`cargo fmt` decides. Use the default `rustfmt` settings, no `rustfmt.toml`, and
no hand-tuning. The value of the standard is that no file negotiates it.

Do not fight it. If a line reads badly after formatting, the fix is usually a
shorter name or an extracted binding, not a manual override.

## Linting

Lints live in the **`[lints]` table in `Cargo.toml`**, not in crate-level
attributes. One place, applied to every target, visible to anyone who reads the
manifest.

**Default clippy must stay at zero.** It catches real defects, not style
preferences. `manual_is_multiple_of`, `needless_range_loop`, and
`manual_checked_division` all fired here, and each one pointed at better code.

**`clippy::pedantic` is not enabled as a whole.** It reports about 93 findings.
Most are `cast_possible_truncation` on deliberate `as u16` casts in layout code,
where a terminal dimension bounds the value. Silencing about 90 of those with
`#[allow]` would teach the reader to skip lint output, which is the opposite of
what the lints are for.

Instead, ten pedantic lints that earn their keep are promoted to warnings in
the manifest: `uninlined_format_args`, `redundant_closure_for_method_calls`,
`needless_pass_by_value`, `unnested_or_patterns`, `match_same_arms`,
`needless_raw_string_hashes`, `explicit_iter_loop`, `implicit_clone`,
`inefficient_to_string`, `manual_let_else`.

**`unsafe_code = "forbid"`** is set under `[lints.rust]`. This codebase has no
`unsafe` and needs none. The compiler enforces that, instead of a document
claiming it.

Still worth running occasionally to see what full pedantic would say:

```bash
cargo clippy --release --all-targets -- -W clippy::pedantic
```

When a lint is genuinely wrong, `#[allow]` it **narrowly**: on the item, never
on the module or the crate, and with a comment that says why. There is one
instance. `poll_realtime` takes owned arguments because it moves onto a detached
thread, so it cannot satisfy `needless_pass_by_value`.

## API and library design

From the API Guidelines, the points that bite in a codebase this size:

- **Naming follows conventions.** `as_` borrows, `to_` clones or converts,
  `into_` consumes. `TestGtfs::into_conn` consumes. `TestGtfs::conn` borrows.
- **Errors carry context.** Use `anyhow::Context` at every boundary where the
  message would otherwise be a bare `NotFound`. An error the user sees must name
  the file, the column, or the URL.
- **Document what the signature does not show**, not what it does. `/// Returns
  the name` on `fn name()` is noise. A note that `arr` can exceed 86400 is not.
- **Take the least specific argument that works.** Prefer `&str` to `&String`,
  `&[T]` to `&Vec<T>`, and `&Path` to `&PathBuf`.
- **Keep visibility tight.** Use `pub` only where another module needs it, and
  `pub(crate)` or private otherwise. `gtfs::create_schema` is `#[cfg(test)]`
  because only fixtures call it.

## Language semantics and the standard library

- **Use `std` first.** `is_multiple_of`, `repeat_n`, `rem_euclid`, `slice::fill`
  and `IsTerminal` each replaced a hand-written version here. If a helper feels
  like it should already exist, check whether it does.
- **No `unsafe`.** The manifest forbids it, so this is a compiler error and not
  a convention.
- **No `unwrap` or `expect` on a path the user can reach.** Test code and real
  invariants are fine. An `expect` must say which invariant was broken.
- **An integer cast is a decision, not a convenience.** Use `i64::from(x)` or
  `try_into()` where the range is not obviously bounded. A bare `as` requires
  that nearby code makes the bound clear.

## Edition

**2024**, set in `Cargo.toml`, with `rust-version = "1.88"` recording the floor.

Our own code needs only 1.87. Edition 2024 needs 1.85, and `is_multiple_of` on
unsigned integers needs 1.87. The dependency tree sets the real floor: `darling`
and the `icu` crates require 1.88.

The MSRV used to be derived from what the code uses, and it was wrong. CI now
holds it against a real 1.88 toolchain, which is how the earlier 1.87 claim was
found to be false. Do not raise or lower this number by reading the source. Let
the CI job say whether it holds.

Migrating from 2021 surfaced one real semantic change. It is worth knowing,
because the same shape will happen again.

`if let Ok(guard) = mutex.lock()` holds the guard for the **whole** `if let`,
including its `else`. Edition 2024 changes when that temporary drops in relation
to other locals. The fix was to give the lock its own scope (`app::with_state`),
not to silence the warning. That is better code under any edition. If you see
`tail-expr-drop-order`, restructure the code. Do not suppress it.

## Project decisions

Recorded so they don't get re-litigated:

- **Unit tests stay in the source file. `tests/` is for integration only.**
  This is Rust's convention, and here the structure also forces it. `otransit`
  is a binary crate with no `src/lib.rs`, so a file in `tests/` compiles as a
  separate crate with nothing to link against. It cannot see our modules at all.
  That is why `tests/terminal.rs` starts the binary instead of calling
  functions.

  A split into `lib.rs` and a thin `main.rs` would let `tests/` import the
  modules. It would also force `pub` onto internals that are private on purpose
  (`match_rank`, `with_state`, `placeholders`), only so that tests can reach
  them. Nothing else uses this code. Do not do it unless a second consumer
  appears.

  A large test module moves to its own child file instead: `app/tests.rs` and
  `ui/tests.rs` are both `mod tests;` in the parent. A child module still sees
  the parent's private items, so this costs no visibility.
- **No `rustfmt.toml`.** Defaults only.
- **Lints belong in `Cargo.toml`**, not in crate attributes.
- **Full pedantic is advisory. The selected ten are gating.** See above.
- **`unsafe_code` is forbidden**, not discouraged.
- **Test-only public items carry `#[cfg(test)]`**, so they do not widen the
  release API and do not trigger `dead_code`.
- **A test module goes last in its file.** Clippy's `items_after_test_module`
  fires otherwise. It has caught real ordering mistakes here twice.
- **Dev-dependencies stay out of the release build.** `tempfile` is under
  `[dev-dependencies]` for this reason.
