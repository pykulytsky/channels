use std::{
    marker::PhantomData,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread::{self, Thread},
};

use crate::utils::queue::Queue;
use crossbeam_epoch::pin;

pub struct Channel<T> {
    queue: Queue<T>,
    ready: AtomicBool,
    senders: AtomicUsize,
}

impl<T> Channel<T> {
    pub fn new() -> Self {
        Self {
            queue: Queue::new(),
            ready: AtomicBool::new(false),
            senders: AtomicUsize::new(0),
        }
    }
}

pub struct Sender<T> {
    channel: Arc<Channel<T>>,
    target_thead: Thread,
}

impl<T> Sender<T> {
    pub fn send(&self, data: T) {
        let guard = &pin();
        self.channel.queue.push(data, &guard);
        self.channel.ready.store(true, Ordering::Release);
        self.target_thead.unpark();
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.channel.senders.fetch_add(1, Ordering::Release);
        Self {
            channel: self.channel.clone(),
            target_thead: thread::current(),
        }
    }
}

impl<T> std::ops::Drop for Sender<T> {
    fn drop(&mut self) {
        self.channel.senders.fetch_sub(1, Ordering::Release);
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

    pub fn recv(&self) -> T {
        let guard = &pin();
        while !self.channel.ready.swap(false, Ordering::Acquire) {
            thread::park();
        }

        self.channel.queue.try_pop(guard).unwrap()
    }

    pub fn try_recv(&self) -> Option<T> {
        let guard = &pin();
        self.channel.queue.try_pop(guard)
    }
}

impl<T> Iterator for Receiver<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let channel = Arc::new(Channel::<T>::new());

    (
        Sender {
            channel: channel.clone(),
            target_thead: thread::current(),
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
    }
}
