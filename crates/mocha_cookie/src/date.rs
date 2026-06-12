//! A tiny parser for the HTTP IMF-fixdate format used by cookie `Expires`.
//!
//! Handles the common form `Wdy, DD Mon YYYY HH:MM:SS GMT` (RFC 7231
//! IMF-fixdate). The two obsolete formats (RFC 850, asctime) are **not**
//! supported; an unparseable date yields `None`, and the cookie is then treated
//! as a session cookie (unless it also has `Max-Age`).

/// Parse an HTTP date into epoch milliseconds (UTC), or `None`.
pub fn parse_http_date_ms(input: &str) -> Option<i64> {
    let s = input.trim();
    // Drop the leading weekday + comma: "Sun, 06 Nov 1994 08:49:37 GMT".
    let rest = match s.find(',') {
        Some(i) => s[i + 1..].trim(),
        None => s,
    };
    let mut parts = rest.split_whitespace();
    let day: i64 = parts.next()?.parse().ok()?;
    let month = month_number(parts.next()?)?;
    let year: i64 = parts.next()?.parse().ok()?;
    let time = parts.next()?;

    let mut hms = time.split(':');
    let hour: i64 = hms.next()?.parse().ok()?;
    let minute: i64 = hms.next()?.parse().ok()?;
    let second: i64 = hms.next()?.parse().ok()?;
    if hour > 23 || minute > 59 || second > 60 || !(1..=31).contains(&day) {
        return None;
    }

    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hour * 3_600 + minute * 60 + second;
    Some(secs * 1000)
}

fn month_number(name: &str) -> Option<i64> {
    Some(match name {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    })
}

/// Days from the Unix epoch (1970-01-01) to `y-m-d`, by Howard Hinnant's
/// `days_from_civil` algorithm. Valid for the proleptic Gregorian calendar.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_date() {
        // 1994-11-06 08:49:37 UTC == 784111777 s.
        assert_eq!(
            parse_http_date_ms("Sun, 06 Nov 1994 08:49:37 GMT"),
            Some(784_111_777_000)
        );
    }

    #[test]
    fn epoch_is_zero() {
        assert_eq!(parse_http_date_ms("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_http_date_ms("not a date"), None);
        assert_eq!(parse_http_date_ms("Sun, 06 Foo 1994 08:49:37 GMT"), None);
    }
}
