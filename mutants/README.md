# Can this suite tell a wrong program from a right one?

A conformance suite is only worth its discriminatory power, and that power is
invisible from inside. Everything can pass while the suite is incapable of
failing. This directory measures it.

```bash
python3 mutants/run.py
```

Each entry in `mutants.py` is one plausible wrong decision a second
implementation could make. The runner applies it, builds, and scores it two
ways.

| | question | what it measures |
|---|---|---|
| **artifact kill** | does `replay conformance` produce different bytes? | whether a port carrying this bug would diff |
| **assertion kill** | does `cargo test` fail? | whether a checked-in assertion names it |

The two come apart, and the gap between them is the point. A mutation the unit
tests kill but the artifact does not is a bug a Go port could carry and ship:
Rust's own suite would catch it in Rust, and nothing would catch it in Go.

**Artifact kill is the number that matters for the port**, because it measures
which seeded divergences the conformance contract can actually reject. It needs
no second implementation, which is why it can be run now.

It is not a completeness guarantee, and the reason is worth being blunt about.
These mutations are a sample of mistakes *their author could imagine*, written
by the same person who built the suite they are scoring. The bugs a Go port
actually produces will cluster elsewhere. They will be idiom mismatches, stdlib
differences in time and text handling, `nil` against `Option`, and goroutine
scheduling. None of these mutations is shaped like that. So the number bounds
nothing in either direction.

What it is good for is comparison with itself. Treat it as a **discrimination
benchmark**, re-run whenever a fixture, a semantic field or a temporal
behaviour changes:

| change | reading |
|---|---|
| 27/33 → 31/33 | the new fixture bought discrimination |
| 27/33 → 27/37 | four mutants added, none detected. Investigate |
| 27/33 → 25/33 | something weakened the contract |
| 27/33 → 19/33 | a fixture was lost, or the artifact stopped carrying something |

## Baseline

**27/33**, measured at `ff57126`. It was 19/33 when the campaign was first run,
and eight fixtures closed the eight gaps that a fixture could close.

Record the commit beside the number. Without one, a later reading has nothing to
compare against: the figure only means something against its own history, and a
history of bare numbers cannot say which suite produced which.

Three stages stand between a mutation and a score:

1. it compiles
2. it changes behaviour
3. something detects the change

Only the third is the score, and stage 2 is why the denominator is *valid
mutations* rather than *mutations written*. A mutation that changes nothing
cannot be detected by anything, and counting it as a survivor understates the
suite.

The runner mechanises stage 2 in one direction. The release binary hashes
stably across rebuilds of identical source, so a mutation whose binary is
unchanged provably changed nothing and is reported as equivalent rather than
scored. The converse does not hold. A different binary is not proof of
different behaviour, because codegen can move without meaning moving. So a
survivor still deserves a look before you believe it.

| | artifact | assertion | count |
|---|---|---|---|
| both | kill | kill | 24 |
| assertion only | **survives** | kill | **4** |
| artifact only | kill | survives | 3 |
| neither | **survives** | **survives** | **2** |

**Artifact kill: 27/33.** Six plausible bugs are invisible to the comparison a
port is judged by, and all six are the same gap.

The three killed by the artifact alone show what differential testing adds.
Nobody wrote an assertion for them, and the artifact caught them anyway. They
are a truncated countdown, a detour drawn on every screen, and one route's
detour shown against all of them.

## What survives

| survivor | what it needs |
|---|---|
| lateness does not wrap past midnight | a slice with `28:xx` trips, and a fixture near 00:30 |
| `mins_until` does not wrap past midnight | " |
| the service day starts at plain midnight | " |
| an after-midnight trip is not marked | " |
| yesterday's service is not consulted | " |
| yesterday's window is dropped from the query | " |

All six are one gap. `cache.db` holds no trip past 24:00 and spans a fortnight
in September, so the service day, the midnight wrap and daylight saving are
unreachable. No fixture *could* walk them. The fix is one change to
`extract.sh` and a re-baseline of every frame, which is why it is its own piece
of work.

The eight that used to sit beside them were reachable. Six fixtures closed all
eight, because two of the six carry two gaps each:

| fixture | what it kills |
|---|---|
| `pinned-stop` | a cancelled pin that keeps its countdown. The board and the pin draw a departure through different arms of one function, and only the board had a fixture. |
| `feed-down` | a first refusal that never reaches the status bar. `cadence` refuses only after a board is on screen, where a refusal is correctly ignored. |
| `pin-onward` | a move that forgets to clear the screen a pin jumped from. `drilldown` escs straight out of the pin, which uses that record and empties it. |
| `weather-night` | an icon code in 30..=39 that is not folded to its day form. An unfolded code draws nothing, which is what an unknown code draws, so `weather-unknown` cannot tell them apart. |
| `filter` | a route found only by its number, and a stop found only by its name. |
| `cadence` | a backoff that survives its own recovery, and a board that is not re-queried as departures go. |

## Reading a result

A survivor is not automatically a gap. Check whether it is *equivalent* first.
An equivalent mutation changed no behaviour, so nothing could have caught it. One entry here
was a comment on a struct field and read as a survivor until it was looked at.
The binary-hash gate would refuse to score that one now. It will not refuse
every such case.

The mutations are Rust-shaped and a Go port cannot reuse them literally. The
categories transfer, and so does `run.py`, which scores by comparing artifacts
rather than by reading source.
