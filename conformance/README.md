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
`type <text>` and `wait <secs>s`. A `#` starts a comment, except inside a
`type`, because a pole number is written `#3009` everywhere in this app.

The `s` on a `wait` is part of the step and every fixture writes it. This file
gave the form as `wait <secs>` until a second implementation wrote a parser from
it and rejected every fixture in the directory.

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
screen adds a frame a port has to match and nothing it can be wrong about. It
usually means the walk was miscounted rather than that the key was meant. If
a step is there to show that a key does nothing, say so in a comment beside it.
Seven such steps were found in four fixtures at once. Six were `enter` on a
departures board, from walks counted one level too deep. The seventh was `/` on
the search screen, which jumps to the screen it is already on.

## The shape of the output

A second implementation has to produce these bytes, so they are written down
here rather than read off a reference binary.

A frame is a blank line, then a header, then every row:

```
<blank>
--- 0. start ---
<row>
<row>
```

The header is `--- ` then the frame number, then `. `, then the step, then
` ---`. Frames count from zero. Frame zero is the first draw and its step is
`start`, because nothing was pressed. Every other label is the step verbatim,
including its argument: `type billings`, `key p`, `wait 30s`.

A run over a directory of fixtures puts a banner before each one:

```
<blank>
======== empty ========
```

Eight `=` on each side, and the fixture's directory name between them. Frame
numbers restart at zero under each banner. A run over a single fixture writes no
banner.

Every row is padded to the full width in runes, including a blank one. A frame
is therefore exactly as many lines as the terminal is rows.

## What a screen is made of

The rows a fixture draws are, in order: a rule, the content, a rule, and the
status bar. The content is anchored to the bottom of its area, so a screen with
two rows of content pads above them and not below.

A terminal too short for all four drops them from the bottom up:

| rows | what is drawn |
|---|---|
| 1 | the top rule |
| 2 | the top rule and one content row |
| 3 | both rules and one content row |
| 4 or more | all four, the content filling what is left |

No fixture is shorter than fourteen rows, so nothing here is reachable from the
suite. It is written down because an implementation has to choose something, and
two implementations choosing differently is a divergence no diff would find.

The status bar is one row. A label comes first, then three spaces, then the
screen's own detail. The hints are right-aligned and end one cell short of the
right edge.

A two-field list row is a marker, the primary label, and a secondary detail in a
dim colour at a fixed column. That column does not move with the width. At 44
cells it still starts where it starts, and the text truncates into it with a
single `…`. At 19 cells it has no room and the detail is not drawn at all.

A row with more fields than two shares out the width instead. Three do, and all
three are plain integer arithmetic rather than a solver, so a port can match
them exactly. `MARKER_W` is 3, `POLE_W` 6, `BADGE_W` 5, `WAIT_W` 10, `NOTE_W` 9.

A **search result** carries a name, a pole code, a destination and a route list:

```
code   = 6
toward = clamp((width * 24) / 100, 10, 22)
routes = clamp((width * 22) / 100, 8, 26)
name   = clamp(width - (3 + 6 + code + toward + routes), 12, 36)
```

A **board reached by search** shares out only its headsign:

```
head = clamp(width - (37 + WAIT_W), 8, 24)
```

A **pin row** shares what is left between the stop's name and its destination:

```
fixed  = 3 + POLE_W + BADGE_W + WAIT_W + NOTE_W + 6
share  = width - fixed
name   = clamp(share * 58 / 100, 10, 28)
toward = clamp(share - name, 6, 20)
```

The name wins the wider half, because the name is what identifies the pin.

Above 71 cells the board's headsign is pinned at 24 and stops moving. The search
row stops moving at 93. The pin row's gaps are irregular where the other two are
uniform, so read the widths from the formula and not from the drawn spaces.

The search row is **not monotonic**, and that is the arithmetic rather than a
mistake. Two independent integer divisions meet a clamp, so the name column goes
12, 13, 12, 13 across widths 48 to 51. Widening the terminal by one cell can make
a column narrower. Reproduce the formula and the widths agree.

No fixture is both narrow and on a search screen, so nothing here is reachable
from the suite. It is written down because an implementation has to choose
something, and a port that invents fixed columns agrees at every width the suite
reaches and diverges everywhere else.

### Two limits, and neither is in the snapshot

A screen draws at most **8 rows**, whatever the terminal height. A search offers
at most **25**, and says `25+` when it stopped there.

Both are display limits. The snapshot reports every row, so a route with 43
stops reports 43 and draws 8. A port that trims the model rather than the
drawing passes the text and fails the semantic.

### The trail, and how it gives way

The status bar's trail loses its **leading** crumbs, one at a time, until what
is left clears the hints by two cells. The last crumb always survives. So a
trail that starts with the mode is a trail that fitted, and a short one has
already given way.

### Two badge forms

A route badge in a row is **five cells**, centred, with the odd space on the
left: `"  1  "`, `"  44 "`. A route badge inside a trail crumb is the name with
one space on each side: `" 1 "`, `" 44 "`. Both are drawn in the route's own
colours.

### The width ladder

The status bar is a label, then the trail crumbs, then any extra. As the width
falls, **the part next to the hints is the one that gives way.** Everything
before it is only clipped by the edge.

Let `target = width - 1 - len(hints)`, which is where the hints prefer to start.

- The last part is cut to `target - col - 1` cells and marked with a single `…`.
  It vanishes when that budget reaches zero.
- Parts between the label and it are dropped whole, one at a time, while
  `col + tailWidth + 2 > target`.
- The hints then start at `max(colAfterTail + 1, target)`, and they **clip with
  no ellipsis**. A row cell at the same edge ellipsizes. The two differ.

Which part gives way therefore depends on the screen. The first screen has a
label and no trail, so its question is what truncates and then disappears. A
route screen has both, so at 19 cells the label is still drawn whole and the
trail is gone. With a filter present the filter outlives the trail, because it
sits last.

The weather banner needs a rule to sit on. It is drawn only when at least one
rule cell survives beside it, which is why 20 cells shows the reading and 19
shows a plain line.

## Names

A stop has more than one written form and the artifact carries two of them.

A station name loses everything from `" O-TRAIN"` onward, so
`HURDMAN O-TRAIN EAST / EST` is `HURDMAN`. That is the form the snapshot
reports and the form a board's status bar shows.

A search result appends the platform when the name does not already end in it.
The same stop therefore draws as `HURDMAN 2`, and `HERON 1A (A)` on platform
`1A` draws as `HERON 1A (A) 1A`. Never read a platform out of a name. Read it
from `platform_code` and append it.

## Colour

Five colours, plus whatever the feed supplies.

| | |
|---|---|
| `#e6e6e6` | text |
| `#6d6e70` | dim, and the second column of a row |
| `#3a3b3d` | the rules, and the quiet parts of the status bar |
| `#da3839` | the cursor marker, always, on every row |
| `#ff6b6b` `#ffc107` `#5cd68a` | the wait ladder below |

A route badge and the tree guide beside a drilled screen are **not** in that
list. They come from `routes.color` and `routes.text_color`, so route 44 is
white on `#0057b8` and the O-Train is white on `#d30f1d`. A port that hardcodes
one colour passes on a slice where every route agrees with it.

### The wait ladder

The wait column is coloured by urgency, and the threshold reads the **rounded
minute** rather than the seconds. A bus 121 seconds away is red, because it says
`2 min`.

| rounded minutes | |
|---|---|
| below zero | dim and struck through |
| 2 or fewer | `#ff6b6b`, bold |
| 6 or fewer | `#ffc107`, bold |
| 15 or fewer | `#5cd68a` |
| more | dim |

Three fixtures pass at the styles level with none of this visible, because every
wait in them is sixteen minutes or more.

### Bold

A selected row is bold from end to end, whichever group it sits in. The cursor
marker is bold on every row, selected or not. A check for "any bold cell on this
row" therefore answers yes everywhere and can never fail.

## Fixed strings

An empty list draws `nothing here`. An empty search draws `no matches`. A board
with nothing left draws `no more departures today`. A pin whose last bus has gone
keeps its row and reads `none left today`. A departure now reads `due`, not
`0 min`. A row borrowed from yesterday's service day is marked `after midnight`.

None of those is derived from anything. A port that invents its own wording
diffs on every frame that reaches one.

## Keys

Entering any screen sets the selection to row 0. That holds in both directions.
A fixture that walks down and escs back finds the cursor at the top, and not
where it left it.

`/` goes to the first screen from a board and from any list. On the stop search
it is an ordinary character, because a stop name can hold one:
`BILLINGS BRIDGE / BANK`.

`esc` clears a filter before it leaves a screen, so two presses leave a filtered
list. Entering a pin records the screen it jumped from, and `esc` honours that
record first. Every other move clears it.

## Regenerating

`extract.sh` rebuilds `cache.db` from a real cache. A newer export will change
the frames. That is a re-baseline, not a failure. Diff the output and look at
what moved.
