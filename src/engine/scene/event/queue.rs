//! Retained event coalescer: latest high-rate state plus ordered discrete edges.

use super::{
    SceneAudioState, SceneEvent, SceneEventSequence, SceneFrameEvents, SceneMediaClockState,
    ScenePointerEvent, ScenePointerEventKind, ScenePointerState, SceneSequencedEvent,
    SceneVideoState,
};

#[derive(Debug, Default)]
pub struct SceneEventQueue {
    next_sequence: u64,
    last_frame_sequence: SceneEventSequence,
    pointer: ScenePointerState,
    audio: SceneAudioState,
    media: Option<SceneMediaClockState>,
    video: Option<SceneVideoState>,
    ordered: Vec<SceneSequencedEvent>,
}

impl SceneEventQueue {
    pub fn publish(&mut self, event: SceneEvent) -> SceneEventSequence {
        self.next_sequence = self.next_sequence.saturating_add(1);
        let sequence = SceneEventSequence(self.next_sequence);
        match event {
            SceneEvent::Pointer(event) => self.publish_pointer(sequence, event),
            SceneEvent::Audio(mut state) => {
                state.sequence = sequence;
                if self.audio_is_current(&state) {
                    self.audio = state;
                }
            }
            SceneEvent::Media(mut state) => {
                state.sequence = sequence;
                if self.media_is_current(&state) {
                    let transition = self.media.is_none_or(|current| {
                        current.session != state.session
                            || current.generation != state.generation
                            || current.playback != state.playback
                            || current.loop_index != state.loop_index
                    });
                    self.media = Some(state);
                    if transition {
                        self.ordered.push(SceneSequencedEvent {
                            sequence,
                            event: SceneEvent::Media(state),
                        });
                    }
                }
            }
            SceneEvent::Video(mut state) => {
                state.sequence = sequence;
                if self.video_is_current(&state) {
                    self.video = Some(state);
                }
            }
        }
        sequence
    }

    pub fn finish_frame(&mut self) -> SceneFrameEvents {
        let last_sequence = SceneEventSequence(self.next_sequence);
        let first_sequence = if last_sequence > self.last_frame_sequence {
            SceneEventSequence(self.last_frame_sequence.0.saturating_add(1))
        } else {
            last_sequence
        };
        self.last_frame_sequence = last_sequence;
        SceneFrameEvents {
            first_sequence,
            last_sequence,
            pointer: self.pointer.clone(),
            audio: self.audio,
            media: self.media,
            video: self.video,
            ordered: std::mem::take(&mut self.ordered),
        }
    }

    fn publish_pointer(&mut self, sequence: SceneEventSequence, event: ScenePointerEvent) {
        self.pointer.sequence = sequence;
        self.pointer.source = event.source;
        self.pointer.surface_id = event.surface_id;
        self.pointer.time_millis = event.time_millis;
        self.pointer.position = event.position;
        self.pointer.surface_size = event.surface_size;
        match event.kind {
            ScenePointerEventKind::Enter { .. } => self.pointer.inside = true,
            ScenePointerEventKind::Leave { .. } => self.pointer.inside = false,
            ScenePointerEventKind::Button {
                button, pressed, ..
            } => {
                update_pressed_buttons(&mut self.pointer.pressed_buttons, button, pressed);
            }
            ScenePointerEventKind::Motion | ScenePointerEventKind::Scroll { .. } => {}
        }
        if !event.kind.is_coalescible() {
            self.ordered.push(SceneSequencedEvent {
                sequence,
                event: SceneEvent::Pointer(event),
            });
        }
    }

    fn audio_is_current(&self, incoming: &SceneAudioState) -> bool {
        match (self.audio.media_session, incoming.media_session) {
            (Some(current), Some(next)) if current == next => {
                incoming.media_generation >= self.audio.media_generation
            }
            _ => true,
        }
    }

    fn media_is_current(&self, incoming: &SceneMediaClockState) -> bool {
        self.media.is_none_or(|current| {
            current.session != incoming.session || incoming.generation >= current.generation
        })
    }

    fn video_is_current(&self, incoming: &SceneVideoState) -> bool {
        self.video.is_none_or(|current| {
            current.session != incoming.session
                || incoming.generation > current.generation
                || (incoming.generation == current.generation
                    && incoming.frame_serial >= current.frame_serial)
        })
    }
}

fn update_pressed_buttons(buttons: &mut Vec<u32>, button: u32, pressed: bool) {
    match buttons.binary_search(&button) {
        Ok(index) if !pressed => {
            buttons.remove(index);
        }
        Err(index) if pressed => buttons.insert(index, button),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::event::{
        SceneMediaGeneration, SceneMediaSessionId, ScenePointerSource,
    };

    fn pointer(kind: ScenePointerEventKind, x: f64) -> SceneEvent {
        SceneEvent::Pointer(ScenePointerEvent {
            source: ScenePointerSource::Replay,
            surface_id: 7,
            time_millis: x as u32,
            position: [x, 20.0],
            surface_size: [100, 100],
            kind,
        })
    }

    #[test]
    fn coalesces_motion_and_preserves_discrete_pointer_order() {
        let mut queue = SceneEventQueue::default();
        queue.publish(pointer(ScenePointerEventKind::Motion, 1.0));
        queue.publish(pointer(ScenePointerEventKind::Motion, 2.0));
        queue.publish(pointer(ScenePointerEventKind::Enter { serial: 3 }, 3.0));
        queue.publish(pointer(
            ScenePointerEventKind::Button {
                button: 0x110,
                pressed: true,
                serial: 4,
            },
            4.0,
        ));
        let frame = queue.finish_frame();
        assert_eq!(frame.pointer.position, [4.0, 20.0]);
        assert_eq!(frame.pointer.pressed_buttons, [0x110]);
        assert_eq!(frame.ordered.len(), 2);
        assert_eq!(frame.ordered[0].sequence, SceneEventSequence(3));
        assert_eq!(frame.ordered[1].sequence, SceneEventSequence(4));
    }

    #[test]
    fn rejects_stale_video_generation_and_serial() {
        let mut queue = SceneEventQueue::default();
        let state = |generation, serial| {
            SceneEvent::Video(SceneVideoState {
                session: SceneMediaSessionId(9),
                generation: SceneMediaGeneration(generation),
                frame_serial: serial,
                ..SceneVideoState::default()
            })
        };
        queue.publish(state(2, 10));
        queue.publish(state(1, 99));
        queue.publish(state(2, 9));
        assert_eq!(queue.finish_frame().video.unwrap().frame_serial, 10);
    }

    #[test]
    fn replaying_the_same_events_produces_the_same_snapshot() {
        let events = [
            pointer(ScenePointerEventKind::Enter { serial: 1 }, 10.0),
            pointer(ScenePointerEventKind::Motion, 25.0),
            pointer(
                ScenePointerEventKind::Scroll {
                    horizontal: 0.0,
                    vertical: -1.0,
                },
                25.0,
            ),
        ];
        let replay = |events: &[SceneEvent]| {
            let mut queue = SceneEventQueue::default();
            for event in events {
                queue.publish(event.clone());
            }
            queue.finish_frame()
        };
        assert_eq!(replay(&events), replay(&events));
    }
}
