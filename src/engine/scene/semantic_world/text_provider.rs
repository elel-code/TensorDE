//! Retained typed text providers evaluated from one local civil-time event snapshot.

use crate::engine::scene::{
    SceneLocalTime, SceneStorage, SceneTextProviderKind, SceneTextProviderRecord,
};

use super::ResolvedTextProviderValue;

#[derive(Debug)]
pub(super) struct RetainedTextProviders {
    providers: Vec<RetainedTextProvider>,
    last_minute: Option<(i32, u8, u8, u8, u8)>,
}

#[derive(Debug)]
struct RetainedTextProvider {
    record: SceneTextProviderRecord,
    initial_text: String,
    source_data: String,
}

impl RetainedTextProviders {
    pub(super) fn from_storage(storage: &SceneStorage) -> Self {
        let providers = storage
            .text_providers()
            .iter()
            .map(|record| RetainedTextProvider {
                record: *record,
                initial_text: storage
                    .string(record.initial_text)
                    .unwrap_or_default()
                    .to_owned(),
                source_data: storage
                    .string(record.source_data)
                    .unwrap_or_default()
                    .to_owned(),
            })
            .collect();
        Self {
            providers,
            last_minute: None,
        }
    }

    pub(super) fn initialize(&self, values: &mut Vec<ResolvedTextProviderValue>) {
        values.clear();
        values.extend(
            self.providers
                .iter()
                .map(|provider| ResolvedTextProviderValue {
                    object: provider.record.object,
                    text: provider.initial_text.clone(),
                }),
        );
    }

    pub(super) fn update(
        &mut self,
        local_time: Option<SceneLocalTime>,
        values: &mut Vec<ResolvedTextProviderValue>,
    ) {
        let Some(time) = local_time else {
            return;
        };
        let minute = (time.year, time.month, time.day, time.hour, time.minute);
        if self.last_minute == Some(minute) {
            return;
        }
        self.last_minute = Some(minute);
        values.clear();
        values.extend(
            self.providers
                .iter()
                .map(|provider| ResolvedTextProviderValue {
                    object: provider.record.object,
                    text: resolve_text(provider.record.kind, &provider.source_data, time),
                }),
        );
    }
}

fn resolve_text(kind: SceneTextProviderKind, source: &str, time: SceneLocalTime) -> String {
    match kind {
        SceneTextProviderKind::ChineseLunarCalendar => chinese_lunar_text(source, time),
        SceneTextProviderKind::ChineseWeekday => {
            let day = ["日", "一", "二", "三", "四", "五", "六"]
                [usize::from(time.weekday_sunday_zero.min(6))];
            format!("周\n{day}")
        }
        SceneTextProviderKind::ChineseSolarTerm => chinese_solar_term(source, time),
        SceneTextProviderKind::ChineseMonthDay => chinese_month_day(time.month, time.day),
        SceneTextProviderKind::ChineseYear => {
            chinese_digits(time.year)
                .chars()
                .fold(String::new(), |mut output, character| {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push(character);
                    output
                })
                + "\n年"
        }
        SceneTextProviderKind::ChineseClock => vertical(&format!(
            "{}点{}分",
            chinese_number(u32::from(time.hour), true),
            chinese_number(u32::from(time.minute), false)
        )),
    }
}

fn chinese_month_day(month: u8, day: u8) -> String {
    let mut lines = chinese_number(u32::from(month), false)
        .chars()
        .map(|character| character.to_string())
        .collect::<Vec<_>>();
    lines.push("月".to_owned());
    lines.extend(
        chinese_number(u32::from(day), false)
            .chars()
            .map(|character| character.to_string()),
    );
    lines.push("日".to_owned());
    lines.join("\n")
}

fn chinese_number(value: u32, hour_uses_two: bool) -> String {
    let digits = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    match value {
        0 => digits[0].to_owned(),
        1..=9 => digits[value as usize].to_owned(),
        10 => "十".to_owned(),
        11..=19 => format!("十{}", digits[(value % 10) as usize]),
        20 => "二十".to_owned(),
        21..=29 => format!("二十{}", digits[(value % 10) as usize]),
        30 => "三十".to_owned(),
        31..=39 => format!("三十{}", digits[(value % 10) as usize]),
        40 => "四十".to_owned(),
        41..=49 => format!("四十{}", digits[(value % 10) as usize]),
        50 => "五十".to_owned(),
        51..=59 => format!("五十{}", digits[(value % 10) as usize]),
        _ => {
            let value = value.to_string();
            value
                .chars()
                .map(|digit| digits[digit.to_digit(10).unwrap_or(0) as usize])
                .collect()
        }
    }
    .replace(
        '二',
        if hour_uses_two && value == 2 {
            "两"
        } else {
            "二"
        },
    )
}

fn chinese_digits(value: i32) -> String {
    let digits = ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
    value
        .unsigned_abs()
        .to_string()
        .chars()
        .map(|digit| digits[digit.to_digit(10).unwrap_or(0) as usize])
        .collect()
}

fn chinese_solar_term(source: &str, time: SceneLocalTime) -> String {
    let Some(block) = year_block(source, time.year) else {
        return " ".to_owned();
    };
    let target = format!("{:02}-{:02}", time.month, time.day);
    let quoted = quoted_values(block);
    for pair in quoted.windows(2) {
        if pair[1].starts_with(&target) {
            return vertical(pair[0]);
        }
    }
    " ".to_owned()
}

fn chinese_lunar_text(source: &str, time: SceneLocalTime) -> String {
    let mut year = time.year;
    let Some(mut data) = lunar_year(source, year) else {
        return lunar_missing_text(time);
    };
    if (time.month, time.day) < (data.spring_month, data.spring_day) {
        year -= 1;
        let Some(previous) = lunar_year(source, year) else {
            return lunar_missing_text(time);
        };
        data = previous;
    }
    let mut remaining = days_from_civil(time.year, time.month, time.day)
        - days_from_civil(year, data.spring_month, data.spring_day);
    let mut months = data.months.clone();
    if data.leap >= 0 && months.len() == 12 {
        let insertion = usize::try_from(data.leap + 1).unwrap_or(months.len());
        if insertion <= months.len() {
            let copied = months[usize::try_from(data.leap).unwrap_or(0).min(11)];
            months.insert(insertion, copied);
        }
    }
    let mut month_index = 0usize;
    while month_index < months.len() && remaining >= i64::from(months[month_index]) {
        remaining -= i64::from(months[month_index]);
        month_index += 1;
    }
    let lunar_day = remaining.max(0) as u8 + 1;
    let month_name = lunar_month_name(month_index, data.leap);
    let year_offset = year - 1984;
    let stems = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"];
    let branches = [
        "子", "丑", "寅", "卯", "辰", "巳", "午", "未", "申", "酉", "戌", "亥",
    ];
    let year_name = format!(
        "{}{}年",
        stems[year_offset.rem_euclid(10) as usize],
        branches[year_offset.rem_euclid(12) as usize]
    );
    two_column_text(
        &format!(
            "{}\n{}",
            vertical(&year_name),
            vertical(&format!("{month_name}{}", lunar_day_name(lunar_day)))
        ),
        &format!(
            "{}\n{}",
            vertical(&format!(
                "{}时",
                branches[usize::from((time.hour + 1) / 2 % 12)]
            )),
            vertical(time_period_name(time.hour, time.minute))
        ),
    )
}

#[derive(Debug)]
struct LunarYear {
    spring_month: u8,
    spring_day: u8,
    months: Vec<u8>,
    leap: i32,
}

fn lunar_year(source: &str, year: i32) -> Option<LunarYear> {
    let block = year_block(source, year)?;
    let spring = block.split("spring:").nth(1)?.split('"').nth(1)?;
    let mut date = spring.split_whitespace().next()?.split('-');
    let _spring_year = date.next()?;
    let spring_month = date.next()?.parse().ok()?;
    let spring_day = date.next()?.parse().ok()?;
    let month_text = block
        .split("months:")
        .nth(1)?
        .split('[')
        .nth(1)?
        .split(']')
        .next()?;
    let months = month_text
        .split(',')
        .filter_map(|value| value.trim().parse().ok())
        .collect::<Vec<_>>();
    let leap = block
        .split("leap:")
        .nth(1)?
        .trim_start()
        .chars()
        .take_while(|character| *character == '-' || character.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()?;
    Some(LunarYear {
        spring_month,
        spring_day,
        months,
        leap,
    })
}

fn year_block(source: &str, year: i32) -> Option<&str> {
    let marker = format!("\"{year}\": {{");
    let tail = source.split_once(&marker)?.1;
    let end = tail.find("},").or_else(|| tail.find("}\n"))?;
    Some(&tail[..end])
}

fn quoted_values(value: &str) -> Vec<&str> {
    let mut values = Vec::new();
    let mut tail = value;
    while let Some(start) = tail.find('"') {
        tail = &tail[start + 1..];
        let Some(end) = tail.find('"') else { break };
        values.push(&tail[..end]);
        tail = &tail[end + 1..];
    }
    values
}

fn lunar_month_name(index: usize, leap: i32) -> String {
    let names = [
        "正", "二", "三", "四", "五", "六", "七", "八", "九", "十", "冬", "腊",
    ];
    if leap >= 0 && index == leap as usize + 1 {
        format!("闰{}月", names[(leap as usize).min(11)])
    } else {
        let adjusted = if leap >= 0 && index > leap as usize + 1 {
            index - 1
        } else {
            index
        };
        format!("{}月", names[adjusted.min(11)])
    }
}

fn lunar_day_name(day: u8) -> String {
    if day == 10 {
        return "初十".to_owned();
    }
    if day == 20 {
        return "二十".to_owned();
    }
    if day == 30 {
        return "三十".to_owned();
    }
    let prefixes = ["初", "十", "廿", "卅"];
    let digits = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    format!(
        "{}{}",
        prefixes[usize::from((day - 1) / 10)],
        digits[usize::from(day % 10)]
    )
}

fn time_period_name(hour: u8, minute: u8) -> &'static str {
    let offset = (u16::from(hour % 2) * 60 + u16::from(minute)) % 60;
    let prefix = if hour % 2 == 0 { "正" } else { "初" };
    match offset {
        0..=14 => {
            if prefix == "正" {
                "正初刻"
            } else {
                "初初刻"
            }
        }
        15..=28 => {
            if prefix == "正" {
                "正一刻"
            } else {
                "初一刻"
            }
        }
        29..=43 => {
            if prefix == "正" {
                "正二刻"
            } else {
                "初二刻"
            }
        }
        44..=57 => {
            if prefix == "正" {
                "正三刻"
            } else {
                "初三刻"
            }
        }
        _ => "小刻",
    }
}

fn lunar_missing_text(time: SceneLocalTime) -> String {
    two_column_text(
        &format!(
            "{}\n{}",
            vertical(&format!("{}年", chinese_digits(time.year))),
            vertical("数据缺失")
        ),
        &format!(
            "{}\n{}",
            vertical("时辰"),
            vertical(time_period_name(time.hour, time.minute))
        ),
    )
}

fn two_column_text(left: &str, right: &str) -> String {
    let left = left.lines().collect::<Vec<_>>();
    let right = right.lines().collect::<Vec<_>>();
    let start = left.len().saturating_sub(right.len()) / 2;
    (0..left.len().max(right.len()))
        .map(|index| {
            format!(
                "{} {}",
                left.get(index).copied().unwrap_or(" "),
                right.get(index.wrapping_sub(start)).copied().unwrap_or(" ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

fn vertical(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| [character, '\n'])
        .collect::<String>()
        .trim_end()
        .to_owned()
}

fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    era * 146097 + year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time() -> SceneLocalTime {
        SceneLocalTime {
            year: 2026,
            month: 7,
            day: 18,
            hour: 9,
            minute: 53,
            weekday_sunday_zero: 6,
        }
    }

    #[test]
    fn simple_chinese_providers_match_authored_script_shapes() {
        assert_eq!(
            resolve_text(SceneTextProviderKind::ChineseWeekday, "", time()),
            "周\n六"
        );
        assert_eq!(
            resolve_text(SceneTextProviderKind::ChineseMonthDay, "", time()),
            "七\n月\n十\n八\n日"
        );
        assert_eq!(
            resolve_text(SceneTextProviderKind::ChineseYear, "", time()),
            "二\n〇\n二\n六\n年"
        );
        assert_eq!(
            resolve_text(SceneTextProviderKind::ChineseClock, "", time()),
            "九\n点\n五\n十\n三\n分"
        );
    }

    #[test]
    fn solar_term_uses_authored_year_table() {
        let source = r#""2026": { "小暑":"07-07 09:56", "大暑":"07-23 03:12" },"#;
        let mut current = time();
        current.day = 23;
        assert_eq!(chinese_solar_term(source, current), "大\n暑");
        current.day = 18;
        assert_eq!(chinese_solar_term(source, current), " ");
    }

    #[test]
    fn lunar_provider_uses_authored_spring_and_month_lengths() {
        let source = r#""2026": { spring: "2026-02-17 00:00", months: [30,29,30,29,29,30,29,29,30,30,30,29], leap: -1 },"#;
        let text = chinese_lunar_text(source, time());
        assert!(text.contains("丙"));
        assert!(text.contains("午"));
        assert!(text.contains("六"));
        assert!(text.contains("月"));
        assert!(text.contains("初"));
        assert!(text.contains("五"));
        assert!(text.contains("巳"));
    }
}
