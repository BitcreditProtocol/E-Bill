use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

pub type DateTimeUtc = DateTime<Utc>;
pub const DEFAULT_DATE_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
pub const DEFAULT_DATE_FORMAT: &str = "%Y-%m-%d";

/// Returns the current time as DateTime
pub fn now() -> DateTimeUtc {
    Utc::now()
}

/// Quickly create a DateTimeUtc from a timestamp. chrone does not
/// really use Results and most of the errors are super unlikely to
/// happen.
pub fn seconds(timestamp: u64) -> DateTimeUtc {
    match Utc.timestamp_opt(timestamp as i64, 0).single() {
        Some(dt) => dt,
        None => panic!("invalid timestamp"),
    }
}

pub fn end_of_day_as_timestamp(timestamp: u64) -> Option<i64> {
    let dt = seconds(timestamp);
    let date = dt.date_naive();
    let end_of_day_time =
        NaiveTime::from_hms_micro_opt(23, 59, 59, 999_999).expect("is a valid time");
    let date_time = date.and_time(end_of_day_time);
    let date_utc = Utc.from_utc_datetime(&date_time);
    Some(date_utc.timestamp())
}

pub fn date_string_to_i64_timestamp(date_str: &str, format_str: Option<&str>) -> Option<i64> {
    let format = format_str.unwrap_or(DEFAULT_DATE_FORMAT);

    let naive_date_time = NaiveDate::parse_from_str(date_str, format)
        .ok()?
        .and_hms_opt(0, 0, 0)?;
    let date_utc = Utc.from_utc_datetime(&naive_date_time);

    Some(date_utc.timestamp())
}

pub fn format_date_string(date: DateTimeUtc) -> String {
    date.format(DEFAULT_DATE_FORMAT).to_string()
}

#[allow(dead_code)]
pub fn date_time_string_to_i64_timestamp(
    date_time_str: &str,
    format_str: Option<&str>,
) -> Option<i64> {
    let format = format_str.unwrap_or(DEFAULT_DATE_TIME_FORMAT);

    let naive_datetime = NaiveDateTime::parse_from_str(date_time_str, format).ok()?;
    let datetime_utc = Utc.from_utc_datetime(&naive_datetime);

    Some(datetime_utc.timestamp())
}

pub fn check_if_deadline_has_passed(
    timestamp_to_check: u64,
    current_timestamp: u64,
    deadline_seconds: u64,
) -> bool {
    // We check this to avoid a u64 underflow, if the block timestamp is in the future, the
    // deadline can't be expired
    if timestamp_to_check > current_timestamp {
        return false;
    }
    let difference = current_timestamp - timestamp_to_check;
    difference > deadline_seconds
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use super::*;
    use chrono::Utc;

    #[test]
    fn test_now() {
        let now = now().timestamp();
        let timestamp = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert!(
            now >= timestamp - 1,
            "now date was {} seconds smaller than expected",
            (timestamp - now)
        );
    }

    #[test]
    fn test_end_of_day() {
        let ts = Utc
            .with_ymd_and_hms(2025, 1, 15, 0, 0, 0)
            .unwrap()
            .timestamp();
        let end_of_day = end_of_day_as_timestamp(ts as u64).unwrap();
        assert!(end_of_day > ts,);
    }

    #[test]
    fn test_date_string_to_i64_timestamp_with_default_format() {
        let date_str = "2025-01-15";
        let expected_timestamp = Utc
            .with_ymd_and_hms(2025, 1, 15, 0, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(
            date_string_to_i64_timestamp(date_str, None),
            Some(expected_timestamp)
        );
    }

    #[test]
    fn test_date_string_to_i64_timestamp_with_custom_format() {
        let date_str = "15/01/2025";
        let expected_timestamp = Utc
            .with_ymd_and_hms(2025, 1, 15, 0, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(
            date_string_to_i64_timestamp(date_str, Some("%d/%m/%Y")),
            Some(expected_timestamp)
        );
    }

    #[test]
    fn test_date_string_to_i64_timestamp_with_invalid_date() {
        assert_eq!(date_string_to_i64_timestamp("2025-32-99", None), None);
        assert_eq!(date_string_to_i64_timestamp("2025/01/15", None), None);
        assert_eq!(date_string_to_i64_timestamp("", None), None);
    }

    #[test]
    fn test_date_time_string_to_i64_timestamp_with_default_format() {
        let date_time_str = "2025-01-15 00:00:00";
        let expected_timestamp = Utc
            .with_ymd_and_hms(2025, 1, 15, 0, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(
            date_time_string_to_i64_timestamp(date_time_str, None),
            Some(expected_timestamp)
        );
    }

    #[test]
    fn test_date_time_string_to_i64_timestamp_with_custom_format() {
        let date_time_str = "15/01/2025 12/30/45";
        let custom_format = "%d/%m/%Y %H/%M/%S";
        let expected_timestamp = Utc
            .with_ymd_and_hms(2025, 1, 15, 12, 30, 45)
            .unwrap()
            .timestamp();
        assert_eq!(
            date_time_string_to_i64_timestamp(date_time_str, Some(custom_format)),
            Some(expected_timestamp)
        );
    }

    #[test]
    fn test_date_time_string_to_i64_timestamp_with_invalid_date() {
        let date_time_str = "2025-13-40 00:00:00";
        assert_eq!(date_time_string_to_i64_timestamp(date_time_str, None), None);
    }

    #[test]
    fn test_date_time_string_to_i64_timestamp_with_invalid_format() {
        let date_time_str = "2025-01-15 00:00:00";
        let invalid_format = "%Q-%X-%Z";
        assert_eq!(
            date_time_string_to_i64_timestamp(date_time_str, Some(invalid_format)),
            None
        );
    }

    #[test]
    fn test_date_time_string_to_i64_timestamp_with_empty_string() {
        let date_time_str = "";
        assert_eq!(date_time_string_to_i64_timestamp(date_time_str, None), None);
    }

    #[test]
    fn test_date_time_string_to_i64_timestamp_with_custom_format_and_empty_string() {
        let date_time_str = "";
        let custom_format = "%d/%m/%Y %H/%M/%S";
        assert_eq!(
            date_time_string_to_i64_timestamp(date_time_str, Some(custom_format)),
            None
        );
    }
}
