//! Process-local civil time snapshot published by the platform event adapter.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneLocalTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub weekday_sunday_zero: u8,
}

impl SceneLocalTime {
    pub fn now() -> Self {
        Self::from_zoned(&jiff::Zoned::now())
    }

    pub fn from_zoned(value: &jiff::Zoned) -> Self {
        let year = i32::from(value.year());
        let month = u8::try_from(value.month()).unwrap_or(1);
        let day = u8::try_from(value.day()).unwrap_or(1);
        Self {
            year,
            month,
            day,
            hour: u8::try_from(value.hour()).unwrap_or(0),
            minute: u8::try_from(value.minute()).unwrap_or(0),
            weekday_sunday_zero: gregorian_weekday_sunday_zero(year, month, day),
        }
    }
}

fn gregorian_weekday_sunday_zero(year: i32, month: u8, day: u8) -> u8 {
    const OFFSETS: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let adjusted_year = year - i32::from(month < 3);
    (adjusted_year + adjusted_year / 4 - adjusted_year / 100
        + adjusted_year / 400
        + OFFSETS[usize::from(month.clamp(1, 12) - 1)]
        + i32::from(day))
    .rem_euclid(7) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gregorian_weekday_matches_known_saturday() {
        assert_eq!(gregorian_weekday_sunday_zero(2026, 7, 18), 6);
    }
}
