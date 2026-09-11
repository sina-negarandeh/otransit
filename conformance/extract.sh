#!/bin/sh
# Rebuild conformance/cache.db from a real cache.
#
# Needs `otransit update` to have run. The slice is deliberate, not arbitrary:
#
#   44 and 48   share TRANSITWAY / TERMINAL and both end at Billings Bridge by
#               roads that do not meet, which is why a pin carries its route
#   44 / 44-1   the same route under two booking periods
#   O-Train 1   rail, which has no realtime at all
#
# Regenerating against a newer export will change the frames. That is a
# re-baseline, not a failure: diff the output and look at what moved.
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
-- Six trips per route and direction: enough for a board to have rows and a
-- direction list to have more than one entry.
INSERT INTO trips
SELECT * FROM real.trips t
WHERE t.service_id IN ('$BUS','$RAIL')
  AND t.route_id IN (SELECT route_id FROM routes)
  AND t.rowid IN (
      SELECT rowid FROM real.trips x
      WHERE x.route_id=t.route_id AND x.direction_id=t.direction_id
        AND x.service_id=t.service_id
      ORDER BY x.trip_id LIMIT 6);
INSERT INTO stop_times SELECT * FROM real.stop_times WHERE trip_id IN (SELECT trip_id FROM trips);
INSERT INTO stops SELECT * FROM real.stops WHERE stop_id IN (SELECT stop_id FROM stop_times);
INSERT INTO meta SELECT * FROM real.meta;
DETACH real;
VACUUM;
SQL
sqlite3 "$OUT" "SELECT 'routes',COUNT(*) FROM routes UNION ALL SELECT 'trips',COUNT(*) FROM trips
  UNION ALL SELECT 'stops',COUNT(*) FROM stops UNION ALL SELECT 'stop_times',COUNT(*) FROM stop_times;"
