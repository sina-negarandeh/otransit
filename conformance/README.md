# Fixed worlds

Everything the app does not control, supplied from a directory. Each fixture
below is one world and one recorded session over it.

```bash
otransit replay conformance > frames.txt
```

That replays every fixture in name order and prints one artifact. The output is
a function of this directory alone. It is the same on any machine, in any
timezone, on any day. That is the point. It is how you compare two builds, or
two implementations of this app.

```bash
otransit-rust/target/release/otransit replay conformance > rust.txt
otransit-go/otransit replay conformance > go.txt
diff rust.txt go.txt
```

The repositories are named apart and the binaries are not. Each is `otransit`
to the person running it, because that is the program's name in both.

## Three observations, compared independently

Each frame can be observed three ways, and none outranks another as a contract.
A semantic difference is reported first only because it usually explains the
other two.

| | what it answers | flag |
|---|---|---|
| text | does a person see the same thing? | *(always)* |
| colours | are the same things emphasised? | `styles` |
| decisions | did the app conclude the same thing? | `semantic` |

The third is inside the second. `feed` carries what the app's polling policy has
done. It carries how many times the app asked, how many of those were refused,
and how long until it asks again. This is for any fixture that drives the
realtime on a clock it controls.

```bash
otransit replay conformance semantic > rust-decisions.txt
```

The semantic block is JSON, one value per line, so a diff points at the decision
that changed rather than at the whole frame. Keys come back sorted: `serde_json`
builds objects on a `BTreeMap`, and Go's `encoding/json` sorts map keys, so
neither side has to do anything to agree on the order.

It is deliberately small. Every name is something a person would notice, and the
snapshot is a list of decisions rather than a dump of the state behind them. A
complete dump would be the Rust structs wearing JSON, which a second
implementation would have to copy rather than agree with. `src/semantic.rs`
records what is left out and why.

```json
{
  "screen": "departures",
  "now": 28800,
  "selected": 0,
  "filter": "",
  "feed": { "note": "live 0s", "requests": 1, "failures": 0, "due_in": 25 },
  "weather": "⛆ light rain · 21°",
  "detour": null,
  "pinned": "unpinned",
  "pins": [],
  "departures": [
    { "route": "44", "headsign": "Hurdman", "scheduled": 32400,
      "live": 32690, "wait": 65, "late": 5,
      "cancelled": false, "after_midnight": false }
  ]
}
```

A departure carries every derivation the board makes from a prediction, because
the derivations are what is under test. The prediction itself is not here. It is in
`rt.json`, which both implementations read. Reading it back through a second
route would risk an answer that disagrees with the one the board used.

## The text output is plain

No escape sequences. A row is what the app drew, character by character.

This matters for a port. Escapes encode how a writer chose to emit a run of
colour, not what the app decided. A second implementation emits the same
colours as different bytes, and a raw diff calls it broken while it is right.

The colours are compared separately:

```bash
otransit replay conformance styles
```

That adds a `styles` block to each frame, one line per row that carries any
colour, as maximal runs:

```
  8  0..3 #da3839+b  3..18 #e6e6e6  18..21 #da3839+b  41..47 #3a3b3d
```

Cells 18 up to 21 are brand red and bold. Maximal runs are canonical. Merge
every neighbour that matches and there is one way to write a row, so this
compares two implementations on the colours they chose and on nothing else.

The letters after `+` are modifiers: `b` bold, `d` dim, `i` italic, `u`
underlined, `r` reversed, `x` crossed out. They are in the artifact because the
app leans on them. A cancelled time is dim and struck through. A bus due in two
minutes is red and bold. A comparison of foreground colour alone lets a port
drop every modifier and still pass.

## Layout

```
conformance/
  cache.db        the schedule, shared by every fixture. See extract.sh.
  <fixture>/
    script        the clock, the timezone, the terminal size, and every key
    rt.json       the realtime predictions
    updates.xml   the published detours
    weather.json  the current conditions
    pins          what is pinned
```

Only `script` is required. The cache is shared because it is the largest thing
here and the least worth copying, and a script names it:

```
cache ../cache.db
```

The feed files are not shared. They are small, and two fixtures that differ in
their weather should differ by holding different files rather than by pointing
at the same one.

## The fixtures

| fixture | what it pins down |
|---|---|
| `drilldown` | the full walk: a pin opened and left, bus to a board and back out, a rail board, a stop search. A detour on both screens below a route, a bus four minutes late, one on time, one cancelled, and one 290 seconds late, where truncating says four minutes and rounding says five. |
| `quiet-feeds` | no realtime, no detours, no weather. It opens the same stop `drilldown` shows as `4 late`, where every row must read `sched`. |
| `platforms` | one stop code over five platforms, and `CANTERBURY / AD. 860`, where the number is an address. Also the only board in the suite whose next departure is the following morning, which is what draws the two-digit hour column `WAIT_W` is sized for. |
| `filter` | `p` is a letter on a list and pins on a board. A route is also found by a word from its long name, and a stop by its pole number. |
| `empty` | a Saturday, when nothing in this slice runs. |
| `after-midnight` | the same Saturday, at 00:00, where the board is Friday's. A trip at 24:04 is one you catch at 00:04, and it belongs to the day that ended. Search cannot reach this board, because today has no service, so the way in is a pin. |
| `narrow` | 44 cells. Truncation, and the order the columns give way in. |
| `too-narrow` | 19 cells. The weather has no room and the rule goes back to being a line. |
| `weather-unknown` | an icon code the app has never seen, and a temperature below zero. |
| `weather-night` | a code in 30..=39, which is the night form of a code in 0..=9 and must fold to it. |
| `pinned-stop` | a pin on a stop rather than a route, drawing a cancelled trip. A board and a pin draw a departure through different arms of one function. |
| `feed-down` | the realtime refuses before it has ever answered, so there is no board to protect and the status bar must say so. |
| `pin-onward` | a move away from a pin, rather than an esc out of it. Every move except the jump into a pin clears the screen the pin came from. |
| `cadence` | the only fixture whose clock moves. The realtime answers, refuses twice, then answers again. The backoff grows and recovers. One wait spans three intervals, and the last one outruns the queue. |

`quiet-feeds` is the one to read first. A broken realtime parse returns no
arrivals and every row reads `sched`, which looks exactly like a quiet Sunday.
A detour feed whose tag convention changed reports nothing, which looks like a
week with none. Both failures are invisible unless something writes down what
"nothing" is supposed to draw.

## What the slice holds

It is chosen, not arbitrary. Routes 44 and 48 share `TRANSITWAY / TERMINAL` and
both end at Billings Bridge by roads that do not meet, which is why a pin
carries its route. Route 44 appears under two booking periods, `44` and `44-1`.
O-Train Line 1 is there because rail has no realtime at all and every row must
read `sched`. Stop code `3034` covers five platforms. The bus service runs
Fridays and the rail service runs weekdays, so a Saturday is empty.

The trips are picked twice over. Six per route and direction give a board its
rows and a direction list more than one entry. Then the two latest that run past
24:00, because a service day reaches 28:xx and a slice that stopped at 23:25
could not reach the arithmetic that handles it. `extract.sh` says why the first
six are ordered the way they are.

Daylight saving is not in here and cannot be. A GTFS feed covers about five
weeks and the changeovers are in March and November, so no export holds one.
`service_day_start` is covered by the unit tests in `app/clock.rs`, which name
the dates, and by nothing in this directory.

## Rules

A feed file that is absent means that feed had nothing to say. A feed file that
is present must parse, and an `updates.xml` that yields no detours is an error
rather than a quiet nothing. Omit the file to mean none. The app is right to be
silent about a feed it could not read. A fixture is the opposite case, because
the whole claim of a directory is that it decides the output.

A replay never writes here. The pins are copied first, so a script that presses
`p` cannot edit the directory it is replaying. Each replay gets its own copy and
removes it at the end, because the test suite replays these directories from
several tests at the same time.

A script sets its world before its first step. A `cache`, `date`, `time`,
`offset` or `size` line after a step is an error. A `time` half-way down looks
like a change of time, and it is not one. It sets the moment the whole run
starts at. Write the offset as `±HH:MM`, which is also how a half-hour zone is
written: `+05:30`.

Steps are `up`, `down`, `enter`, `esc`, `backspace`, `key <char>`,
`type <text>` and `wait <secs>`. A `#` starts a comment, except inside a `type`,
because a pole number is written `#3009` everywhere in this app.

`wait` is the only step that is not a keypress. It moves the clock, which is how
a departure goes by, a countdown moves, and the realtime cadence comes round.
Every other fixture holds its clock still.

A `feed` line queues what the realtime answers with: `feed ok <file>` or
`feed fail <message>`. It belongs to the header, not to the steps, because a
script does not get to say *when* the app asks. The queue is taken in order
whenever the app's own policy says an attempt is due. A port that polls twice as
often runs out of answers and one that never polls leaves them unused, and the
frames say so either way. Naming the moments instead would compare two
implementations against a timetable this file invented, rather than against each
other.

A `wait` covers every interval it spans, not just the last one. Ninety seconds
at a twenty-five second cadence is three attempts, each recorded at the moment
it came due rather than at the end of the wait. A wait is at most a day: the
clock is seconds into a service day, and an unbounded one overflows it.

A fixture with `feed` lines does not also load `rt.json` at the start. The queue
owns the realtime, and its first answer lands before the first frame is drawn.
Loading the file as well would give the opening board two sources with the queue
silently winning.

Running out of answers is not an error. The attempt stays owed, `due_in` holds
at zero, and the request count stops climbing. That is what a feed gone quiet
looks like from inside the app, and it is worth being able to compare.

Row 0 of the first screen is a pin when the fixture has one, and a mode when it
does not. Two fixtures therefore do not share their steps. A script that
hardcoded the row would walk somewhere else and still print frames that diff
cleanly.

A script names what it selects. Write `type hurdman` before the `enter` that
picks a direction, not a bare `enter` that takes whichever one sorts first. The
slice decides that order, so a new trip moves it. This is not hypothetical:
widening the slice sent one fixture to a different board, every test stayed
green, and a checked-in mutation stopped being detected. The same applies to any
list a script walks through.

Every step must change the frame. A keypress that redraws what is already on
screen adds a frame a port has to match and nothing it can be wrong about, and
it usually means the walk was miscounted rather than that the key was meant. If
a step is there to show that a key does nothing, say so in a comment beside it.
Seven such steps were found in four fixtures at once. Six were `enter` on a
departures board, from walks counted one level too deep. The seventh was `/` on
the search screen, which jumps to the screen it is already on.

## Regenerating

`extract.sh` rebuilds `cache.db` from a real cache. A newer export will change
the frames. That is a re-baseline, not a failure. Diff the output and look at
what moved.
