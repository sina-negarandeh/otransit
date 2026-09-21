// One held instant, and where it sits in the service day.
//
// A service day is not a calendar day. Its origin is noon minus twelve hours,
// which is midnight on almost every day and the correct answer on the two days
// a year that are not twenty-four hours long. Wall-clock arithmetic reports
// 08:00 as 28800 seconds on every day, and on a day that lost an hour the true
// elapsed time is 25200.
//
// Nothing here reads the machine. The caller supplies the instant and the zone,
// so a test names both and a running program passes what it read.

import Foundation

public struct Clock: Sendable, Equatable {
    /// Noon on the service date. It fixes the date and the weekday, and it is
    /// not read off `start`: the origin of a day that lost an hour falls on the
    /// day before, so a date read from there names the wrong day.
    private let day: Date
    /// The service day origin, and the held instant.
    private let start: Date
    private let at: Date
    private let calendar: Calendar

    /// Ottawa. The schedule is written against this zone and no other, so a
    /// machine set to UTC must still draw the board a rider is standing at.
    public static let ottawa = TimeZone(identifier: "America/Toronto")!

    /// The clock at one instant, on the service day that instant falls in.
    ///
    /// This is the only place a service day is worked out, and nothing here
    /// reads the machine: a test hands in an instant it named and a running
    /// program hands in one it read, and no screen can tell which.
    public init(at instant: Date, zone: TimeZone = Clock.ottawa) {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = zone
        let parts = cal.dateComponents([.year, .month, .day], from: instant)
        let noon = cal.date(
            from: DateComponents(year: parts.year, month: parts.month, day: parts.day, hour: 12))!

        self.calendar = cal
        self.day = noon
        self.start = noon.addingTimeInterval(-12 * 3600)
        self.at = instant
    }

    /// The instant that `date` and `hhmm` name in `zone`, written YYYY-MM-DD
    /// and HH:MM. Parsed by hand rather than by a formatter, because a
    /// formatter reads the machine's locale and these two shapes are fixed.
    public init?(date: String, hhmm: String, zone: TimeZone = Clock.ottawa) {
        let ymd = date.split(separator: "-").map { Int($0) }
        let hm = hhmm.split(separator: ":").map { Int($0) }
        guard ymd.count == 3, hm.count == 2,
            let y = ymd[0], let m = ymd[1], let d = ymd[2],
            let hour = hm[0], let minute = hm[1]
        else { return nil }

        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = zone
        guard
            let instant = cal.date(
                from: DateComponents(year: y, month: m, day: d, hour: hour, minute: minute))
        else { return nil }
        self.init(at: instant, zone: zone)
    }

    /// The held instant, in seconds since the service day began. It can exceed
    /// 86400 on a day that gained an hour, and a schedule reaches 28:xx for its
    /// own reasons, so nothing may assume it is under a day.
    public var now: Int { Int(at.timeIntervalSince(start)) }

    /// The service date, written YYYYMMDD, which is how the cache stores it.
    public var date: String { Self.ymd(day, calendar) }

    /// The service date before this one. The step back is taken from noon,
    /// which exists on every day, so a day that lost an hour cannot land it on
    /// the wrong date.
    public var yesterday: String {
        Self.ymd(calendar.date(byAdding: .day, value: -1, to: day)!, calendar)
    }

    /// The held instant in seconds since 1970. A realtime feed stamps itself in
    /// absolute time and a schedule is written in seconds on a service day;
    /// this and `second(of:)` are the only place either counting crosses.
    public var epoch: Int { Int(at.timeIntervalSince1970) }

    /// Where an absolute instant falls on this service day. It reads the origin
    /// and not midnight, so a prediction on a day that lost an hour lands on
    /// the same axis a schedule does.
    public func second(of epoch: Int) -> Int { epoch - Int(start.timeIntervalSince1970) }

    /// The same service day, at a stated time of day, written HH:MM.
    ///
    /// Built from noon, which is on the service date by construction, so this
    /// cannot land on the day before on a day that lost an hour.
    public func at(_ hhmm: String) -> Clock? {
        let parts = hhmm.split(separator: ":").map { Int($0) }
        guard parts.count == 2, let hour = parts[0], let minute = parts[1],
            let moment = calendar.date(
                bySettingHour: hour, minute: minute, second: 0, of: day)
        else { return nil }
        return Clock(at: moment, zone: calendar.timeZone)
    }

    /// The clock `seconds` real seconds later. The service day does not move:
    /// a board is drawn for the day it was opened on.
    public func advanced(by seconds: Int) -> Clock {
        Clock(at: at.addingTimeInterval(TimeInterval(seconds)), zone: calendar.timeZone)
    }

    private static func ymd(_ d: Date, _ cal: Calendar) -> String {
        let p = cal.dateComponents([.year, .month, .day], from: d)
        return String(format: "%04d%02d%02d", p.year!, p.month!, p.day!)
    }
}
