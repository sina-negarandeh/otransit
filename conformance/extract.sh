#!/bin/sh
# Rebuild conformance/cache.db from a real cache.
#
# Needs `otransit update` to have run. The slice is deliberate, not arbitrary:
#
#   44 and 48   share TRANSITWAY / TERMINAL and both end at Billings Bridge by
#               roads that do not meet, which is why a pin carries its route
#   44 / 44-1   the same route under two booking periods
#   O-Train 1   rail, which has no realtime at all
#   past 24:00  trips that belong to the previous service day, so a board at
#               00:30 has something on it and `after_midnight` has a row
#
# The trips are chosen twice over. Six per route and direction give a board its
# rows and a direction list more than one entry. Those six are ordered by
# `trip_id`, which is arbitrary and reaches neither end of the day. Then the
# latest two that run past 24:00, because a service day reaches 28:xx and a
# slice that stopped at 23:25 could not reach the arithmetic that handles it.
#
# Regenerating against a newer export will change the frames. That is a
# re-baseline, not a failure: diff the output and look at what moved.
#
# Daylight saving is not reachable from any export, whatever this script picks.
# README.md says why, under "What the slice holds".
set -eu
REAL="${1:-$HOME/Library/Caches/otransit/gtfs.db}"
OUT="$(dirname "$0")/cache.db"
BUS=11-1
RAIL=SEPT26-CFDFri-Weekday-03-1

rm -f "$OUT"
sqlite3 "$REAL" .schema | grep -v sqlite_stat | sqlite3 "$OUT"
sqlite3 "$OUT" <<SQL
ATTACH '$REAL' AS real;
INSERT INTO routes SELECT * FROM real.routes WHERE short_name IN ('1','44','48');
INSERT INTO calendar SELECT * FROM real.calendar WHERE service_id IN ('$BUS','$RAIL');

INSERT INTO trips
SELECT * FROM real.trips t
WHERE t.service_id IN ('$BUS','$RAIL')
  AND t.route_id IN (SELECT route_id FROM routes)
  AND (
    -- Six per route and direction, by id. Arbitrary, and left that way on
    -- purpose: it is what the checked-in frames were baselined against, and
    -- re-ordering it would move every fixture for no gain. Ordering by start
    -- time instead was tried and gave the six earliest, which left the stop the
    -- fixtures use with nothing between 06:52 and midnight.
    t.rowid IN (
      SELECT rowid FROM real.trips x
      WHERE x.route_id = t.route_id AND x.direction_id = t.direction_id
        AND x.service_id = t.service_id
      ORDER BY x.trip_id LIMIT 6)
    -- And the two latest that run past midnight, which the six above reach only
    -- by luck and did not. The outer WHERE has already cut this to the three
    -- routes the slice keeps, so the aggregate reads their stop_times and not
    -- most of the table.
    OR t.trip_id IN (
      SELECT st.trip_id FROM real.stop_times st
      JOIN real.trips x ON x.trip_id = st.trip_id
      WHERE x.route_id = t.route_id AND x.direction_id = t.direction_id
        AND x.service_id = t.service_id
      GROUP BY st.trip_id HAVING MAX(st.arr) >= 86400
      ORDER BY MAX(st.arr) DESC LIMIT 2)
  );
INSERT INTO stop_times SELECT * FROM real.stop_times WHERE trip_id IN (SELECT trip_id FROM trips);
INSERT INTO stops SELECT * FROM real.stops WHERE stop_id IN (SELECT stop_id FROM stop_times);
INSERT INTO meta SELECT * FROM real.meta;
DETACH real;
VACUUM;
SQL
sqlite3 "$OUT" "SELECT 'routes',COUNT(*) FROM routes UNION ALL SELECT 'trips',COUNT(*) FROM trips
  UNION ALL SELECT 'stops',COUNT(*) FROM stops UNION ALL SELECT 'stop_times',COUNT(*) FROM stop_times
  UNION ALL SELECT 'past 24:00',COUNT(*) FROM stop_times WHERE arr >= 86400;"
