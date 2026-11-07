//! A semaphore with a short queue (docs 05 section 7).
//!
//! Proof verification and attestation reports are the gate's costly CPU
//! work. Each runs behind one of these: a fixed number at once, a few
//! more waiting, and anyone past that refused at once, so a flood turns
//! into fast `503`s instead of a growing backlog.

use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::{Semaphore, SemaphorePermit};

/// A semaphore that turns callers away when its queue is full.
#[derive(Debug)]
pub struct Admission {
    permits: Semaphore,
    waiting: AtomicUsize,
    max_waiting: usize,
}

impl Admission {
    /// `concurrent` permits, with at most `max_waiting` callers queued for one.
    pub fn new(concurrent: usize, max_waiting: usize) -> Self {
        Self { permits: Semaphore::new(concurrent), waiting: AtomicUsize::new(0), max_waiting }
    }

    /// A permit, queueing if there's room; `None` if the queue is full.
    pub async fn enter(&self) -> Option<SemaphorePermit<'_>> {
        if let Ok(permit) = self.permits.try_acquire() {
            return Some(permit);
        }
        let max = self.max_waiting;
        self.waiting.fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| (n < max).then_some(n + 1)).ok()?;
        let _place = QueuePlace(&self.waiting);
        self.permits.acquire().await.ok()
    }

    /// A permit, however long the queue. For work already promised to a
    /// wallet, such as producing a refund at close.
    pub async fn wait(&self) -> SemaphorePermit<'_> {
        self.permits.acquire().await.expect("the semaphore is never closed")
    }
}

/// A place in the queue, given up when the wait ends or is cancelled.
struct QueuePlace<'a>(&'a AtomicUsize);

impl Drop for QueuePlace<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[tokio::test]
    async fn refuses_past_the_queue() {
        let admission = Arc::new(Admission::new(1, 1));
        let held = admission.enter().await.unwrap();

        let queued = tokio::spawn({
            let admission = Arc::clone(&admission);
            async move { admission.enter().await.is_some() }
        });
        while admission.waiting.load(Ordering::Acquire) == 0 {
            tokio::task::yield_now().await;
        }
        assert!(admission.enter().await.is_none(), "the queue holds one");

        drop(held);
        assert!(queued.await.unwrap());
        assert_eq!(admission.waiting.load(Ordering::Acquire), 0);
        assert!(admission.enter().await.is_some());
    }
}
