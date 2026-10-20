//! Exchange clock: trading days, session hours and bot phases for NSE (IST)
//! and NYSE (US Eastern), independent of the machine's own time zone.
//!
//! Everything here is a pure function of a UTC instant, so callers pass
//! `Utc::now()` and tests pass fixed times.
//!
//! * IST is a fixed +05:30 offset (India has no daylight saving).
//! * US Eastern applies the US daylight-saving rule (second Sunday of March
//!   to the first Sunday of November, switching at 02:00 local). That rule is
//!   small enough to compute here instead of pulling in a time-zone database.
//!
//! # Updating the holiday tables
//!
//! NSE publishes next year's trading holidays in a circular every December
//! ("Holidays for the year …", equity segment). NYSE publishes three years at
//! a time. To update:
//!
//! 1. Add the new rows to [`NSE_HOLIDAYS`] / [`NYSE_HOLIDAYS`] (weekday
//!    closures only — weekend holidays don't change anything) and any 1:00 pm
//!    early closes to [`NYSE_EARLY_CLOSES`].
//! 2. Move the year from [`NSE_PROVISIONAL_YEARS`] (if it was there) into
//!    [`NSE_OFFICIAL_YEARS`], or extend [`NYSE_OFFICIAL_YEARS`].
//! 3. Run `cargo test clock` — the tests check that every row is a weekday
//!    inside a covered year.
//!
//! Years outside the tables fall back to "weekdays are trading days", and
//! [`calendar_coverage`] reports that so commands can warn about it.

use crate::market::Market;
use chrono::{
    DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveTime, TimeZone, Utc, Weekday,
};

/// Bot session phases, derived from the exchange's open and close times.
///
/// For NSE (09:15–15:30 IST) the boundaries are 09:00, 09:15, 09:30, 14:00,
/// 15:15, 15:20 and 16:00. NYSE uses the same offsets from 09:30 and 16:00 ET
/// (or 13:00 on early-close days).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Phase {
    PreMarket,  // open − 15 min
    Opening,    // open → open + 15 min: the entry window
    Active,     // until close − 90 min
    WindDown,   // no new entries, stops tighten
    SquareOff,  // close − 15 min: close everything
    PostMarket, // close − 10 min → close + 30 min: settle
    Closed,     // anything else, weekends and holidays
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Phase::PreMarket => write!(f, "Pre-Market"),
            Phase::Opening => write!(f, "Opening"),
            Phase::Active => write!(f, "Active Trading"),
            Phase::WindDown => write!(f, "Wind-Down"),
            Phase::SquareOff => write!(f, "Square-Off"),
            Phase::PostMarket => write!(f, "Post-Market"),
            Phase::Closed => write!(f, "Closed"),
        }
    }
}

/// NSE equity-segment trading holidays that fall on weekdays.
///
/// 2026: NSE circular of December 2025 (15 weekday holidays).
/// 2027: provisional — only the fixed-date national holidays that fall on a
/// weekday. NSE had not published the 2027 list when this table was written;
/// replace these rows with the official list when it is out.
pub const NSE_HOLIDAYS: &[(i32, u32, u32, &str)] = &[
    (2026, 1, 26, "Republic Day"),
    (2026, 3, 3, "Holi"),
    (2026, 3, 26, "Shri Ram Navami"),
    (2026, 3, 31, "Shri Mahavir Jayanti"),
    (2026, 4, 3, "Good Friday"),
    (2026, 4, 14, "Dr. Baba Saheb Ambedkar Jayanti"),
    (2026, 5, 1, "Maharashtra Day"),
    (2026, 5, 28, "Bakri Id"),
    (2026, 6, 26, "Muharram"),
    (2026, 9, 14, "Ganesh Chaturthi"),
    (2026, 10, 2, "Mahatma Gandhi Jayanti"),
    (2026, 10, 20, "Dussehra"),
    (2026, 11, 10, "Diwali Balipratipada"),
    (2026, 11, 24, "Prakash Gurpurb Sri Guru Nanak Dev"),
    (2026, 12, 25, "Christmas"),
    (2027, 1, 26, "Republic Day"),
    (2027, 4, 14, "Dr. Baba Saheb Ambedkar Jayanti"),
];

/// Years whose NSE list above is the official circular.
pub const NSE_OFFICIAL_YEARS: &[i32] = &[2026];
/// Years with only the fixed-date holidays filled in.
pub const NSE_PROVISIONAL_YEARS: &[i32] = &[2027];

/// NYSE full-day closures (NYSE Group calendar of December 2025).
pub const NYSE_HOLIDAYS: &[(i32, u32, u32, &str)] = &[
    (2026, 1, 1, "New Year's Day"),
    (2026, 1, 19, "Martin Luther King, Jr. Day"),
    (2026, 2, 16, "Washington's Birthday"),
    (2026, 4, 3, "Good Friday"),
    (2026, 5, 25, "Memorial Day"),
    (2026, 6, 19, "Juneteenth"),
    (2026, 7, 3, "Independence Day (observed)"),
    (2026, 9, 7, "Labor Day"),
    (2026, 11, 26, "Thanksgiving Day"),
    (2026, 12, 25, "Christmas Day"),
    (2027, 1, 1, "New Year's Day"),
    (2027, 1, 18, "Martin Luther King, Jr. Day"),
    (2027, 2, 15, "Washington's Birthday"),
    (2027, 3, 26, "Good Friday"),
    (2027, 5, 31, "Memorial Day"),
    (2027, 6, 18, "Juneteenth (observed)"),
    (2027, 7, 5, "Independence Day (observed)"),
    (2027, 9, 6, "Labor Day"),
    (2027, 11, 25, "Thanksgiving Day"),
    (2027, 12, 24, "Christmas Day (observed)"),
    (2028, 1, 17, "Martin Luther King, Jr. Day"),
    (2028, 2, 21, "Washington's Birthday"),
    (2028, 4, 14, "Good Friday"),
    (2028, 5, 29, "Memorial Day"),
    (2028, 6, 19, "Juneteenth"),
    (2028, 7, 4, "Independence Day"),
    (2028, 9, 4, "Labor Day"),
    (2028, 11, 23, "Thanksgiving Day"),
    (2028, 12, 25, "Christmas Day"),
];

/// NYSE days that close at 1:00 pm ET.
pub const NYSE_EARLY_CLOSES: &[(i32, u32, u32)] = &[
    (2026, 11, 27),
    (2026, 12, 24),
    (2027, 11, 26),
    (2028, 7, 3),
    (2028, 11, 24),
];

pub const NYSE_OFFICIAL_YEARS: &[i32] = &[2026, 2027, 2028];

/// How much of a year's holiday calendar is bundled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarCoverage {
    /// The exchange's published list.
    Official,
    /// Only fixed-date holidays; festival dates may be missing.
    Provisional,
    /// No list: every weekday is treated as a trading day.
    Missing,
}

pub fn calendar_coverage(market: Market, year: i32) -> CalendarCoverage {
    let (official, provisional): (&[i32], &[i32]) = match market {
        Market::In => (NSE_OFFICIAL_YEARS, NSE_PROVISIONAL_YEARS),
        Market::Us => (NYSE_OFFICIAL_YEARS, &[]),
    };
    if official.contains(&year) {
        CalendarCoverage::Official
    } else if provisional.contains(&year) {
        CalendarCoverage::Provisional
    } else {
        CalendarCoverage::Missing
    }
}

/// A one-line warning when `year`'s holiday list is incomplete, else None.
pub fn coverage_warning(market: Market, year: i32) -> Option<String> {
    match calendar_coverage(market, year) {
        CalendarCoverage::Official => None,
        CalendarCoverage::Provisional => Some(format!(
            "The {} holiday list for {} is provisional (only fixed-date holidays) — \
             upgrade StockWise once the exchange publishes it.",
            exchange(market),
            year
        )),
        CalendarCoverage::Missing => Some(format!(
            "No {} holiday list for {} is bundled — every weekday is treated as a \
             trading day. Upgrade StockWise for the current calendar.",
            exchange(market),
            year
        )),
    }
}

/// Exchange name used in messages.
pub fn exchange(market: Market) -> &'static str {
    match market {
        Market::In => "NSE",
        Market::Us => "NYSE",
    }
}

/// Time-zone label used in messages.
pub fn tz_label(market: Market) -> &'static str {
    match market {
        Market::In => "IST",
        Market::Us => "ET",
    }
}

pub fn ist() -> FixedOffset {
    FixedOffset::east_opt(5 * 3600 + 30 * 60).expect("valid offset")
}

fn hm(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).expect("valid time")
}

/// The `n`th (1-based) `weekday` of `month`.
fn nth_weekday(year: i32, month: u32, weekday: Weekday, n: u32) -> NaiveDate {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid date");
    let shift = (7 + weekday.num_days_from_monday() - first.weekday().num_days_from_monday()) % 7;
    first + Duration::days((shift + 7 * (n - 1)) as i64)
}

/// Whether US daylight saving time is in effect on `date` (local), using the
/// rule in force since 2007. Market hours never fall in the 02:00 changeover
/// hour, so the date alone decides.
fn us_dst_on(date: NaiveDate) -> bool {
    let start = nth_weekday(date.year(), 3, Weekday::Sun, 2);
    let end = nth_weekday(date.year(), 11, Weekday::Sun, 1);
    date >= start && date < end
}

fn us_offset_for_local_date(date: NaiveDate) -> FixedOffset {
    let hours = if us_dst_on(date) { -4 } else { -5 };
    FixedOffset::east_opt(hours * 3600).expect("valid offset")
}

/// US Eastern offset at a UTC instant (EDT −4 / EST −5).
pub fn us_eastern_offset(utc: DateTime<Utc>) -> FixedOffset {
    let year = utc.year();
    // Changes happen at 02:00 local: 07:00 UTC in March (EST), 06:00 UTC in
    // November (EDT).
    let start = nth_weekday(year, 3, Weekday::Sun, 2)
        .and_time(hm(7, 0))
        .and_utc();
    let end = nth_weekday(year, 11, Weekday::Sun, 1)
        .and_time(hm(6, 0))
        .and_utc();
    let hours = if utc >= start && utc < end { -4 } else { -5 };
    FixedOffset::east_opt(hours * 3600).expect("valid offset")
}

/// The exchange's UTC offset at an instant.
pub fn offset(market: Market, utc: DateTime<Utc>) -> FixedOffset {
    match market {
        Market::In => ist(),
        Market::Us => us_eastern_offset(utc),
    }
}

/// `utc` in the exchange's local time.
pub fn local(market: Market, utc: DateTime<Utc>) -> DateTime<FixedOffset> {
    utc.with_timezone(&offset(market, utc))
}

/// The exchange-local calendar date at `utc` — the date a session belongs to.
pub fn session_date(market: Market, utc: DateTime<Utc>) -> NaiveDate {
    local(market, utc).date_naive()
}

/// Exchange-local wall time on `date` as a UTC instant.
pub fn at(market: Market, date: NaiveDate, time: NaiveTime) -> DateTime<Utc> {
    let off = match market {
        Market::In => ist(),
        Market::Us => us_offset_for_local_date(date),
    };
    off.from_local_datetime(&date.and_time(time))
        .single()
        .expect("fixed offsets are unambiguous")
        .with_timezone(&Utc)
}

fn table(market: Market) -> &'static [(i32, u32, u32, &'static str)] {
    match market {
        Market::In => NSE_HOLIDAYS,
        Market::Us => NYSE_HOLIDAYS,
    }
}

/// The holiday's name if the exchange is closed for a holiday on `date`.
pub fn holiday(market: Market, date: NaiveDate) -> Option<&'static str> {
    table(market)
        .iter()
        .find(|(y, m, d, _)| *y == date.year() && *m == date.month() && *d == date.day())
        .map(|(_, _, _, name)| *name)
}

/// A weekday that is not an exchange holiday.
pub fn is_trading_day(market: Market, date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun) && holiday(market, date).is_none()
}

/// Regular-session open and close (exchange-local) on a trading day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hours {
    pub open: NaiveTime,
    pub close: NaiveTime,
}

/// Session hours on `date`, or None when the exchange is closed all day.
pub fn hours(market: Market, date: NaiveDate) -> Option<Hours> {
    if !is_trading_day(market, date) {
        return None;
    }
    Some(match market {
        Market::In => Hours {
            open: hm(9, 15),
            close: hm(15, 30),
        },
        Market::Us => {
            let early = NYSE_EARLY_CLOSES
                .iter()
                .any(|(y, m, d)| *y == date.year() && *m == date.month() && *d == date.day());
            Hours {
                open: hm(9, 30),
                close: if early { hm(13, 0) } else { hm(16, 0) },
            }
        }
    })
}

/// Bot phase at `utc` (see [`Phase`]).
pub fn market_phase(market: Market, utc: DateTime<Utc>) -> Phase {
    let now = local(market, utc);
    let Some(h) = hours(market, now.date_naive()) else {
        return Phase::Closed;
    };
    let t = now.time();
    let m = |mins: i64| Duration::minutes(mins);
    if t < h.open - m(15) || t >= h.close + m(30) {
        Phase::Closed
    } else if t < h.open {
        Phase::PreMarket
    } else if t < h.open + m(15) {
        Phase::Opening
    } else if t < h.close - m(90) {
        Phase::Active
    } else if t < h.close - m(15) {
        Phase::WindDown
    } else if t < h.close - m(10) {
        Phase::SquareOff
    } else {
        Phase::PostMarket
    }
}

/// True during the regular session (open ≤ t < close).
pub fn is_open(market: Market, utc: DateTime<Utc>) -> bool {
    let now = local(market, utc);
    hours(market, now.date_naive()).is_some_and(|h| now.time() >= h.open && now.time() < h.close)
}

/// When the regular session closes on `date` (UTC), if it trades.
pub fn close_at(market: Market, date: NaiveDate) -> Option<DateTime<Utc>> {
    hours(market, date).map(|h| at(market, date, h.close))
}

/// When positions are squared off on `date`: 15 minutes before the close.
pub fn square_off_at(market: Market, date: NaiveDate) -> Option<DateTime<Utc>> {
    hours(market, date).map(|h| at(market, date, h.close - Duration::minutes(15)))
}

/// The next regular-session open at or after `utc` (today's if it is still
/// ahead). Looks up to 30 days ahead, far longer than any exchange closure.
pub fn next_open(market: Market, utc: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let today = session_date(market, utc);
    (0..30).find_map(|i| {
        let date = today + Duration::days(i);
        let open = at(market, date, hours(market, date)?.open);
        (open >= utc).then_some(open)
    })
}

/// "Wed 21 Oct 09:15 IST" for messages.
pub fn format_local(market: Market, utc: DateTime<Utc>) -> String {
    format!(
        "{} {}",
        local(market, utc).format("%a %d %b %H:%M"),
        tz_label(market)
    )
}

/// Why the exchange is closed at `utc`, for messages: "Dussehra",
/// "weekend", or "outside market hours".
pub fn closed_reason(market: Market, utc: DateTime<Utc>) -> String {
    let date = session_date(market, utc);
    if let Some(name) = holiday(market, date) {
        format!("holiday: {}", name)
    } else if matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
        "weekend".into()
    } else {
        "outside market hours".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn holiday_tables_list_weekdays_in_covered_years() {
        for (market, rows) in [(Market::In, NSE_HOLIDAYS), (Market::Us, NYSE_HOLIDAYS)] {
            for (y, m, d, name) in rows {
                let day = date(*y, *m, *d);
                assert!(
                    !matches!(day.weekday(), Weekday::Sat | Weekday::Sun),
                    "{day} {name} is a weekend"
                );
                assert_ne!(
                    calendar_coverage(market, *y),
                    CalendarCoverage::Missing,
                    "{day} is in an uncovered year"
                );
            }
        }
        for (y, m, d) in NYSE_EARLY_CLOSES {
            let day = date(*y, *m, *d);
            assert!(
                is_trading_day(Market::Us, day),
                "{day} early close on a closed day"
            );
        }
    }

    #[test]
    fn nse_2026_has_fifteen_weekday_holidays() {
        let n = NSE_HOLIDAYS.iter().filter(|(y, ..)| *y == 2026).count();
        assert_eq!(n, 15);
        assert_eq!(holiday(Market::In, date(2026, 10, 20)), Some("Dussehra"));
        assert!(!is_trading_day(Market::In, date(2026, 10, 20)));
        assert!(is_trading_day(Market::In, date(2026, 10, 21)));
        assert!(!is_trading_day(Market::In, date(2026, 10, 24))); // Saturday
    }

    #[test]
    fn ist_is_independent_of_the_local_zone() {
        // 03:45 UTC = 09:15 IST, the NSE open.
        let t = utc("2026-10-21T03:45:00Z");
        assert_eq!(local(Market::In, t).time(), hm(9, 15));
        assert!(is_open(Market::In, t));
        assert!(!is_open(Market::In, utc("2026-10-21T03:44:59Z")));
        assert!(!is_open(Market::In, utc("2026-10-21T10:00:00Z"))); // 15:30 IST
        // 20:00 UTC on the 20th is already the 21st in India.
        assert_eq!(
            session_date(Market::In, utc("2026-10-20T20:00:00Z")),
            date(2026, 10, 21)
        );
    }

    #[test]
    fn nse_phases_follow_the_bot_schedule() {
        let at_ist = |hh: u32, mm: u32| at(Market::In, date(2026, 10, 21), hm(hh, mm));
        assert_eq!(market_phase(Market::In, at_ist(8, 59)), Phase::Closed);
        assert_eq!(market_phase(Market::In, at_ist(9, 0)), Phase::PreMarket);
        assert_eq!(market_phase(Market::In, at_ist(9, 15)), Phase::Opening);
        assert_eq!(market_phase(Market::In, at_ist(9, 30)), Phase::Active);
        assert_eq!(market_phase(Market::In, at_ist(13, 59)), Phase::Active);
        assert_eq!(market_phase(Market::In, at_ist(14, 0)), Phase::WindDown);
        assert_eq!(market_phase(Market::In, at_ist(15, 15)), Phase::SquareOff);
        assert_eq!(market_phase(Market::In, at_ist(15, 20)), Phase::PostMarket);
        assert_eq!(market_phase(Market::In, at_ist(15, 59)), Phase::PostMarket);
        assert_eq!(market_phase(Market::In, at_ist(16, 0)), Phase::Closed);
    }

    #[test]
    fn holidays_are_closed_all_day() {
        let t = at(Market::In, date(2026, 10, 20), hm(11, 0));
        assert_eq!(market_phase(Market::In, t), Phase::Closed);
        assert_eq!(closed_reason(Market::In, t), "holiday: Dussehra");
        let sat = at(Market::In, date(2026, 10, 24), hm(11, 0));
        assert_eq!(closed_reason(Market::In, sat), "weekend");
    }

    #[test]
    fn next_open_skips_weekends_and_holidays() {
        // Monday 19 Oct after the close → Tuesday is Dussehra → Wednesday 09:15.
        let t = at(Market::In, date(2026, 10, 19), hm(16, 0));
        let open = next_open(Market::In, t).unwrap();
        assert_eq!(open, at(Market::In, date(2026, 10, 21), hm(9, 15)));
        assert_eq!(format_local(Market::In, open), "Wed 21 Oct 09:15 IST");
        // Friday evening → Monday.
        let fri = at(Market::In, date(2026, 10, 23), hm(18, 0));
        assert_eq!(
            next_open(Market::In, fri).unwrap(),
            at(Market::In, date(2026, 10, 26), hm(9, 15))
        );
        // Before today's open → today.
        let early = at(Market::In, date(2026, 10, 21), hm(7, 0));
        assert_eq!(
            next_open(Market::In, early).unwrap(),
            at(Market::In, date(2026, 10, 21), hm(9, 15))
        );
    }

    #[test]
    fn us_eastern_tracks_daylight_saving() {
        // 2026: DST from Sun 8 Mar to Sun 1 Nov.
        assert_eq!(nth_weekday(2026, 3, Weekday::Sun, 2), date(2026, 3, 8));
        assert_eq!(nth_weekday(2026, 11, Weekday::Sun, 1), date(2026, 11, 1));
        // Summer: 13:30 UTC = 09:30 EDT.
        let summer = utc("2026-07-01T13:30:00Z");
        assert_eq!(local(Market::Us, summer).time(), hm(9, 30));
        assert!(is_open(Market::Us, summer));
        // Winter: 14:30 UTC = 09:30 EST.
        let winter = utc("2026-01-15T14:30:00Z");
        assert_eq!(local(Market::Us, winter).time(), hm(9, 30));
        assert!(is_open(Market::Us, winter));
        assert!(!is_open(Market::Us, utc("2026-01-15T13:30:00Z")));
        // Changeover instants.
        assert_eq!(
            us_eastern_offset(utc("2026-03-08T06:59:00Z")).local_minus_utc(),
            -5 * 3600
        );
        assert_eq!(
            us_eastern_offset(utc("2026-03-08T07:00:00Z")).local_minus_utc(),
            -4 * 3600
        );
        assert_eq!(
            us_eastern_offset(utc("2026-11-01T05:59:00Z")).local_minus_utc(),
            -4 * 3600
        );
        assert_eq!(
            us_eastern_offset(utc("2026-11-01T06:00:00Z")).local_minus_utc(),
            -5 * 3600
        );
    }

    #[test]
    fn nyse_holidays_and_early_closes() {
        assert!(!is_trading_day(Market::Us, date(2026, 11, 26)));
        let h = hours(Market::Us, date(2026, 11, 27)).unwrap();
        assert_eq!(h.close, hm(13, 0));
        // 12:50 ET on an early-close day is already post-market.
        let t = at(Market::Us, date(2026, 11, 27), hm(12, 50));
        assert_eq!(market_phase(Market::Us, t), Phase::PostMarket);
        let normal = at(Market::Us, date(2026, 11, 30), hm(12, 50));
        assert_eq!(market_phase(Market::Us, normal), Phase::Active);
        assert_eq!(
            square_off_at(Market::Us, date(2026, 11, 30)).unwrap(),
            at(Market::Us, date(2026, 11, 30), hm(15, 45))
        );
    }

    #[test]
    fn coverage_reports_missing_years() {
        assert_eq!(
            calendar_coverage(Market::In, 2026),
            CalendarCoverage::Official
        );
        assert_eq!(
            calendar_coverage(Market::In, 2027),
            CalendarCoverage::Provisional
        );
        assert_eq!(
            calendar_coverage(Market::In, 2030),
            CalendarCoverage::Missing
        );
        assert!(coverage_warning(Market::In, 2026).is_none());
        assert!(coverage_warning(Market::Us, 2030).unwrap().contains("NYSE"));
        // An uncovered year still treats weekdays as trading days.
        assert!(is_trading_day(Market::In, date(2030, 1, 2)));
    }
}
