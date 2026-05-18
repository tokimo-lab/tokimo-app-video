use std::fmt;
use chrono::{DateTime, TimeZone};

pub trait ApiDateTimeExt {
    type Output;
    fn to_api_datetime(&self) -> Self::Output;
}

impl<Tz> ApiDateTimeExt for DateTime<Tz>
where Tz: TimeZone, Tz::Offset: fmt::Display {
    type Output = String;
    fn to_api_datetime(&self) -> String { self.to_rfc3339() }
}

impl<Tz> ApiDateTimeExt for Option<DateTime<Tz>>
where Tz: TimeZone, Tz::Offset: fmt::Display {
    type Output = Option<String>;
    fn to_api_datetime(&self) -> Option<String> { self.as_ref().map(DateTime::to_rfc3339) }
}

pub trait OptionalApiDateTimeExt {
    fn to_api_datetime_or_default(&self) -> String;
}

impl<Tz> OptionalApiDateTimeExt for Option<DateTime<Tz>>
where Tz: TimeZone, Tz::Offset: fmt::Display {
    fn to_api_datetime_or_default(&self) -> String {
        self.as_ref().map(DateTime::to_rfc3339).unwrap_or_default()
    }
}
