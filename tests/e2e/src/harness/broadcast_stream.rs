//! Async Observation Stream Ring Buffer with Drop-Oldest Backpressure.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use crate::harness::driver_mock::TvEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequencedEvent {
    pub seq: u64,
    pub event: TvEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamItem {
    Event(SequencedEvent),
    Lagged(u64), // number of events skipped due to drop-oldest
}

pub struct DropOldestRingBuffer {
    capacity: usize,
    buffer: Arc<Mutex<VecDeque<SequencedEvent>>>,
    next_seq: AtomicU64,
    total_dropped: AtomicU64,
    tx: broadcast::Sender<SequencedEvent>,
}

impl DropOldestRingBuffer {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity.max(16));
        Self {
            capacity,
            buffer: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            next_seq: AtomicU64::new(1),
            total_dropped: AtomicU64::new(0),
            tx,
        }
    }

    pub async fn push(&self, event: TvEvent) -> SequencedEvent {
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        let item = SequencedEvent { seq, event };

        let mut buf = self.buffer.lock().await;
        if buf.len() >= self.capacity {
            buf.pop_front();
            self.total_dropped.fetch_add(1, Ordering::SeqCst);
        }
        buf.push_back(item.clone());

        // Broadcast; if receivers are lagged or zero, broadcast error is ignored
        let _ = self.tx.send(item.clone());
        item
    }

    pub fn total_dropped(&self) -> u64 {
        self.total_dropped.load(Ordering::SeqCst)
    }

    pub fn current_seq(&self) -> u64 {
        self.next_seq.load(Ordering::SeqCst) - 1
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SequencedEvent> {
        self.tx.subscribe()
    }
}
