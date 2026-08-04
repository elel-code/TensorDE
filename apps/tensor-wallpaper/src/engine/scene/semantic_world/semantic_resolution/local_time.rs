//! Local civil-time change classification for retained SceneScript dispatch.

use crate::engine::scene::{SceneLocalTime, SceneScriptSubscriptions};

pub(super) fn local_time_dirty_events(
    local_time: SceneLocalTime,
    last_minute: &mut Option<(i32, u8, u8, u8, u8)>,
    last_second: &mut Option<(i32, u8, u8, u8, u8, u8)>,
) -> SceneScriptSubscriptions {
    let minute = (
        local_time.year,
        local_time.month,
        local_time.day,
        local_time.hour,
        local_time.minute,
    );
    let second = (
        local_time.year,
        local_time.month,
        local_time.day,
        local_time.hour,
        local_time.minute,
        local_time.second,
    );
    let mut dirty = SceneScriptSubscriptions::NONE;
    if *last_minute != Some(minute) {
        dirty = dirty.union(SceneScriptSubscriptions::LOCAL_TIME);
        *last_minute = Some(minute);
    }
    if *last_second != Some(second) {
        dirty = dirty.union(SceneScriptSubscriptions::LOCAL_TIME_SECOND);
        *last_second = Some(second);
    }
    dirty
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_transition_dirties_second_clock_without_dirtying_minute_clock() {
        let mut minute = None;
        let mut second = None;
        let at = |minute, second| SceneLocalTime {
            year: 2026,
            month: 7,
            day: 29,
            hour: 12,
            minute,
            second,
            weekday_sunday_zero: 3,
        };
        let first = local_time_dirty_events(at(22, 23), &mut minute, &mut second);
        assert!(first.contains(SceneScriptSubscriptions::LOCAL_TIME));
        assert!(first.contains(SceneScriptSubscriptions::LOCAL_TIME_SECOND));
        assert_eq!(
            local_time_dirty_events(at(22, 24), &mut minute, &mut second),
            SceneScriptSubscriptions::LOCAL_TIME_SECOND
        );
        let next_minute = local_time_dirty_events(at(23, 0), &mut minute, &mut second);
        assert!(next_minute.contains(SceneScriptSubscriptions::LOCAL_TIME));
        assert!(next_minute.contains(SceneScriptSubscriptions::LOCAL_TIME_SECOND));
    }
}
