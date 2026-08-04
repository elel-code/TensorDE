use super::*;

#[test]
fn configure_queue_is_bounded_and_defers_the_latest_size() {
    let mut queue = ConfigureQueue::new();
    for serial in 1..=MAX_PENDING_CONFIGURES as u32 {
        assert!(queue.push(serial, (serial, serial)).is_some());
    }
    assert!(queue.push(99, (99, 99)).is_none());
    assert_eq!(queue.len, MAX_PENDING_CONFIGURES);
    assert_eq!(queue.ack(MAX_PENDING_CONFIGURES as u32), Ok(Some((99, 99))));
    assert_eq!(queue.len, 0);
}

#[test]
fn configure_ack_consumes_the_selected_serial_and_older_entries() {
    let mut queue = ConfigureQueue::new();
    assert!(queue.push(4, (10, 10)).is_some());
    assert!(queue.push(5, (20, 20)).is_some());
    assert!(queue.push(6, (30, 30)).is_some());
    assert_eq!(queue.ack(5), Ok(None));
    assert_eq!(queue.len, 1);
    assert_eq!(queue.pending[0].map(|configure| configure.serial), Some(6));
    assert_eq!(queue.ack(5), Err(()));
}
