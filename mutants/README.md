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
| 30/33 → 32/33 | the new fixture bought discrimination |
| 30/33 → 30/37 | four mutants added, none detected. Investigate |
| 30/33 → 28/33 | something weakened the contract |
| 30/33 → 19/33 | a fixture was lost, or the artifact stopped carrying something |

Every row of that table reads the total, and the total can hide the thing you
are looking for. Widening the slice added a direction to one list, and
`drilldown` reached its board by pressing enter on whichever direction sorted
first. It walked to a different board, took the 290-second row with it, and
*lateness truncates instead of rounding* stopped being detected by anything.
Every test still passed.

The total went **up** through that, from 27/33 to 29/33, because the same change
closed three other gaps. A regression can sit inside a gain, and no reading of
the total shows it. So compare the per-mutation outcomes the run prints, not
only the number at the end, and re-run on any change to the slice.

## Baseline

**30/33**, artifact `494f2f8dd019`. That is twelve characters of **sha1**:

```bash
otransit replay conformance styles semantic | shasum | cut -c1-12
```

Name the algorithm beside the number. A bare digest was published here once, a
second implementation measured the same bytes with `shasum -a 256`, and the
mismatch read exactly like a moving reference. It cost an afternoon of chasing a
binary that had never moved.

It was 19/33 when the campaign was first
run. Eight fixtures took it to 27/33, and a slice reaching past 24:00 took it to
30/33. The 27/33 reading was measured at commit `ff57126`.

Record the artifact digest beside the number. `run.py` prints it on the first
line of every run, and it is what the score was actually measured against: the
bytes of `replay conformance styles semantic`. A commit hash was used before and
names the wrong thing twice over. It moves when a squash or a rebase rewrites
the branch the measurement was taken on, and it does not move when a commit
lands that leaves the suite alone. A bare number is worse than either, because
the figure only means something against its own history, and a history of bare
numbers cannot say which suite produced which.

The suite asserts that digest too, in `conformance/mod.rs`, so a fixture cannot
move without somebody accepting the move. `run.py` skips that one test by name,
and the skip is load-bearing. The test hashes the artifact, so counted as an
assertion it would fail for every mutation that moves the artifact, every
artifact kill would read as an assertion kill, and the gap between the two
columns would close by construction. Both files say so at the line that does it.

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
| both | kill | kill | 27 |
| assertion only | **survives** | kill | **3** |
| artifact only | kill | survives | 3 |
| neither | **survives** | **survives** | **0** |

**Artifact kill: 30/33.** Three plausible bugs are invisible to the comparison a
port is judged by. None of the three is reachable from a fixture.

The three killed by the artifact alone show what differential testing adds.
Nobody wrote an assertion for them, and the artifact caught them anyway. They
are a truncated countdown, a detour drawn on every screen, and one route's
detour shown against all of them.

## What survives

| survivor | why no fixture reaches it |
|---|---|
| the service day starts at plain midnight | Daylight saving is not in any GTFS export, so no slice of one holds a changeover. `conformance/README.md` says why. `app/clock.rs` names the two dates in a unit test, because nothing else can. |
| `mins_until` does not wrap past midnight | The board cannot hold a departure an hour behind its clock. `departures` asks for `arr >= now + shift` and stores `arr - shift`, so every scheduled row is at or after `now` on one axis. |
| lateness does not wrap past midnight | The same axis, from the other side. A prediction is read through `Clock::service_secs` against the day the row was shifted onto, so the two cannot be a day apart. |

The last two are guards against a feed that contradicts the timetable by hours.
A fixture could invent one. It would then be baselining an answer nobody has
decided is right: a prediction ninety minutes stale reads as "22h 0 min" under
the guard and as "-90 min" without it, and the guard is the one this app calls
correct only because midnight is the case it was written for. Leave them to the
unit tests until a real feed produces one.

The six that used to sit beside them were reachable. A wider slice closed three:
`extract.sh` now takes the two latest trips per route and direction that run
past 24:00, and `after-midnight` walks a board at 00:00 on a day with no service
of its own. That closed *an after-midnight trip is not marked*, *yesterday's
service is not consulted*, and *yesterday's window is dropped from the query*.

Eight more were closed before that, by six fixtures, because two of the six
carry two gaps each:

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
