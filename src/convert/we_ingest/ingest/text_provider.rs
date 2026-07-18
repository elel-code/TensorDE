//! SceneScript text update classification into typed, non-JavaScript providers.

use serde_json::Value;

use crate::engine::scene::SceneTextProviderKind;

use super::super::ir::WeIrTextProvider;

pub(super) fn text_provider(
    object: u32,
    value: &Value,
    initial_text: &str,
) -> Option<WeIrTextProvider> {
    let script = value.get("text")?.get("script")?.as_str()?;
    let kind = classify_text_provider(script)?;
    let source_data = matches!(
        kind,
        SceneTextProviderKind::ChineseLunarCalendar | SceneTextProviderKind::ChineseSolarTerm
    )
    .then(|| script.to_owned())
    .unwrap_or_default();
    Some(WeIrTextProvider {
        object,
        kind,
        initial_text: initial_text.to_owned(),
        source_data,
        update_interval_seconds: 60,
    })
}

fn classify_text_provider(script: &str) -> Option<SceneTextProviderKind> {
    if script.contains("lunarData") && script.contains("getFullLunarDate") {
        Some(SceneTextProviderKind::ChineseLunarCalendar)
    } else if script.contains("solarTerms") && script.contains("getCurrentSolarTermOrFestival") {
        Some(SceneTextProviderKind::ChineseSolarTerm)
    } else if script.contains("getDay()") && script.contains("weekdays") {
        Some(SceneTextProviderKind::ChineseWeekday)
    } else if script.contains("getMonth()")
        && script.contains("getDate()")
        && script.contains("convertToChinese")
    {
        Some(SceneTextProviderKind::ChineseMonthDay)
    } else if script.contains("getFullYear().toString()") && script.contains("chineseYear") {
        Some(SceneTextProviderKind::ChineseYear)
    } else if script.contains("getHours()")
        && script.contains("getMinutes()")
        && script.contains("hourChinese")
    {
        Some(SceneTextProviderKind::ChineseClock)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_supported_calendar_scripts_without_layer_names() {
        assert_eq!(
            classify_text_provider("weekdays[now.getDay()]"),
            Some(SceneTextProviderKind::ChineseWeekday)
        );
        assert_eq!(
            classify_text_provider(
                "getMonth(); getDate(); const convertToChinese = value => value"
            ),
            Some(SceneTextProviderKind::ChineseMonthDay)
        );
        assert_eq!(
            classify_text_provider("getFullYear().toString(); let chineseYear = ''"),
            Some(SceneTextProviderKind::ChineseYear)
        );
        assert_eq!(
            classify_text_provider("getHours(); getMinutes(); const hourChinese = []"),
            Some(SceneTextProviderKind::ChineseClock)
        );
    }

    #[test]
    fn calendar_tables_remain_typed_source_data_only_when_required() {
        let value = serde_json::json!({
            "text": {"script": "const lunarData={}; getFullLunarDate()"}
        });
        let provider = text_provider(4, &value, "fallback").expect("provider");
        assert_eq!(provider.kind, SceneTextProviderKind::ChineseLunarCalendar);
        assert!(!provider.source_data.is_empty());
    }
}
