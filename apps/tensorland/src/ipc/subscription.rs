//! Bounded compositor-owned registry for value-only IPC event streams.

use futures_channel::mpsc;

use super::{EventMessage, EventTopic, ServerEvent};

pub(crate) const MAX_IPC_SUBSCRIBERS: usize = 64;
const MAX_PENDING_EVENTS_PER_SUBSCRIBER: usize = 8;

pub(crate) struct IpcSubscriptionSink {
    topics: Vec<EventTopic>,
    events: mpsc::Sender<EventMessage>,
}

pub(crate) fn subscription_channel(
    topics: Vec<EventTopic>,
) -> (IpcSubscriptionSink, mpsc::Receiver<EventMessage>) {
    let (events, receiver) = mpsc::channel(MAX_PENDING_EVENTS_PER_SUBSCRIBER);
    (IpcSubscriptionSink { topics, events }, receiver)
}

pub(crate) struct IpcSubscriptions {
    subscribers: Vec<IpcSubscriptionSink>,
    next_sequence: u64,
}

impl IpcSubscriptions {
    pub(crate) fn new() -> Self {
        Self {
            subscribers: Vec::with_capacity(MAX_IPC_SUBSCRIBERS),
            next_sequence: 1,
        }
    }

    pub(crate) fn register(&mut self, sink: IpcSubscriptionSink) -> Result<(), ()> {
        self.subscribers
            .retain(|subscriber| !subscriber.events.is_closed());
        if self.subscribers.len() == MAX_IPC_SUBSCRIBERS {
            return Err(());
        }
        self.subscribers.push(sink);
        Ok(())
    }

    pub(crate) fn publish(&mut self, topic: EventTopic, event: ServerEvent) -> PublishSummary {
        let message = EventMessage::new(self.take_sequence(), event);
        let mut delivered = 0;
        let mut dropped = 0;
        self.subscribers.retain_mut(|subscriber| {
            if !subscriber.topics.contains(&topic) {
                return true;
            }
            match subscriber.events.try_send(message.clone()) {
                Ok(()) => {
                    delivered += 1;
                    true
                }
                Err(_) => {
                    dropped += 1;
                    false
                }
            }
        });
        PublishSummary { delivered, dropped }
    }

    fn take_sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.checked_add(1).unwrap_or(1);
        sequence
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct PublishSummary {
    pub(crate) delivered: usize,
    pub(crate) dropped: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{ConfigReloadEvent, ConfigReloadEventResult};

    fn reload_event(request_id: u64) -> ServerEvent {
        ServerEvent::ConfigReload(ConfigReloadEvent {
            request_id,
            generation: request_id,
            result: ConfigReloadEventResult::Applied,
        })
    }

    #[test]
    fn registry_sequences_and_delivers_only_matching_topics() {
        let mut subscriptions = IpcSubscriptions::new();
        let (sink, mut events) = subscription_channel(vec![EventTopic::ConfigReload]);
        subscriptions.register(sink).unwrap();

        assert_eq!(
            subscriptions.publish(EventTopic::ConfigReload, reload_event(9)),
            PublishSummary {
                delivered: 1,
                dropped: 0,
            }
        );
        let message = events.try_recv().unwrap();
        assert_eq!(message.sequence, 1);
        assert_eq!(message.event, reload_event(9));
    }

    #[test]
    fn disconnected_subscriber_is_pruned_before_capacity_check() {
        let mut subscriptions = IpcSubscriptions::new();
        for _ in 0..MAX_IPC_SUBSCRIBERS {
            let (sink, receiver) = subscription_channel(vec![EventTopic::ConfigReload]);
            drop(receiver);
            subscriptions.register(sink).unwrap();
        }
        let (sink, _receiver) = subscription_channel(vec![EventTopic::ConfigReload]);
        assert_eq!(subscriptions.register(sink), Ok(()));
    }

    #[test]
    fn live_subscriber_capacity_is_fixed() {
        let mut subscriptions = IpcSubscriptions::new();
        let mut receivers = Vec::new();
        for _ in 0..MAX_IPC_SUBSCRIBERS {
            let (sink, receiver) = subscription_channel(vec![EventTopic::ConfigReload]);
            subscriptions.register(sink).unwrap();
            receivers.push(receiver);
        }
        let (sink, _receiver) = subscription_channel(vec![EventTopic::ConfigReload]);
        assert_eq!(subscriptions.register(sink), Err(()));
        assert_eq!(receivers.len(), MAX_IPC_SUBSCRIBERS);
    }

    #[test]
    fn slow_subscriber_is_removed_when_its_fixed_queue_fills() {
        let mut subscriptions = IpcSubscriptions::new();
        let (sink, _receiver) = subscription_channel(vec![EventTopic::ConfigReload]);
        subscriptions.register(sink).unwrap();

        let mut dropped = 0;
        for request_id in 0..=(MAX_PENDING_EVENTS_PER_SUBSCRIBER as u64 + 1) {
            dropped += subscriptions
                .publish(EventTopic::ConfigReload, reload_event(request_id))
                .dropped;
        }
        assert_eq!(dropped, 1);
        assert_eq!(
            subscriptions.publish(EventTopic::ConfigReload, reload_event(99)),
            PublishSummary::default()
        );
    }
}
