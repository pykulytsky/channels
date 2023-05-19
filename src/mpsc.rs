use std::{
    marker::PhantomData,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use crate::utils::queue::Queue;
use crossbeam_epoch::pin;

pub struct Channel<T> {
    queue: Queue<T>,
    ready: AtomicBool,
}

impl<T> Channel<T> {
    pub fn new() -> Self {
        Self {
            queue: Queue::new(),
            ready: AtomicBool::new(false),
        }
    }
}

pub struct Sender<T> {
    channel: Arc<Channel<T>>,
}

impl<T> Sender<T> {
    pub fn send(&self, data: T) {
        let guard = &pin();
        self.channel.queue.push(data, &guard);
        self.channel.ready.store(true, Ordering::Release);
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            channel: self.channel.clone(),
        }
    }
}

pub struct Receiver<T> {
    channel: Arc<Channel<T>>,
    _no_send: PhantomData<*const ()>,
}

impl<T> Receiver<T> {
    pub fn ready(&self) -> bool {
        self.channel.ready.load(Ordering::Acquire)
    }
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let channel = Arc::new(Channel::<T>::new());

    (
        Sender {
            channel: channel.clone(),
        },
        Receiver {
            channel: channel.clone(),
            _no_send: PhantomData,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let (tx, rx) = channel();
        tx.send(1);
        assert!(rx.ready());
        assert_eq!(rx.recv(), 1);
        assert!(rx.try_recv().is_none());
        tx.send(1);
        assert!(rx.try_recv().is_some());
        let tx1 = tx.clone();
        tx1.send(1);
        assert!(rx.try_recv().is_some());
    }
}
