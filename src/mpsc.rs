use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use crate::utils::{
    queue::Queue,
    wait::{wait, wake_one},
};
use crossbeam_epoch::pin;

/// Field ready indicates wheater receiver need to block, in order to receive new message using
/// `recv`.
pub struct Channel<T> {
    queue: Queue<T>,
    messages: AtomicUsize,
    senders: AtomicUsize,
}

impl<T> Channel<T> {
    pub fn new() -> Self {
        Self {
            queue: Queue::new(),
            messages: AtomicUsize::new(0),
            senders: AtomicUsize::new(1),
        }
    }
}

pub struct Sender<T> {
    channel: Arc<Channel<T>>,
}

impl<T> Sender<T> {
    #[inline]
    pub fn send(&self, data: T) {
        let guard = &pin();
        self.channel.queue.push(data, guard);
        self.channel.messages.fetch_add(1, Ordering::Release);
        wake_one(&self.channel.messages);
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.channel.senders.fetch_add(1, Ordering::Release);
        Self {
            channel: self.channel.clone(),
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
}

impl<T> Receiver<T> {
    #[inline]
    pub fn ready(&self) -> bool {
        self.channel.messages.load(Ordering::Acquire) > 0
    }

    #[inline]
    fn senders_remaining(&self) -> usize {
        self.channel.senders.load(Ordering::Acquire)
    }

    #[inline]
    pub fn recv(&self) -> T {
        let guard = &pin();
        let mut messages = self.channel.messages.load(Ordering::Acquire);
        loop {
            if messages > 0 {
                break loop {
                    match self.channel.messages.compare_exchange(
                        messages,
                        messages - 1,
                        Ordering::Release,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => break,
                        Err(v) => {
                            messages = v;
                        }
                    }
                };
            } else {
                wait(&self.channel.messages, messages);
            }
        }

        self.channel.queue.try_pop(guard).unwrap()
    }

    #[inline]
    pub fn try_recv(&self) -> Option<T> {
        if self.senders_remaining() < 1 && self.channel.messages.load(Ordering::Acquire) < 1 {
            return None;
        }
        let guard = &pin();
        self.channel.queue.try_pop(guard)
    }
}

pub struct IntoIter<T> {
    rx: Receiver<T>,
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        if self.rx.senders_remaining() < 1 && self.rx.channel.messages.load(Ordering::Acquire) < 1 {
            return None;
        }
        Some(self.rx.recv())
    }
}

impl<T> IntoIterator for Receiver<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter { rx: self }
    }
}

pub struct Iter<'a, T> {
    rx: &'a Receiver<T>,
}

impl<T> Iterator for Iter<'_, T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        if self.rx.senders_remaining() < 1 && self.rx.channel.messages.load(Ordering::Acquire) < 1 {
            return None;
        }
        Some(self.rx.recv())
    }
}

impl<'a, T> IntoIterator for &'a Receiver<T> {
    type Item = T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        Iter { rx: self }
    }
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let channel = Arc::new(Channel::<T>::new());

    (
        Sender {
            channel: channel.clone(),
        },
        Receiver { channel },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::thread;

    #[test]
    fn it_works() {
        let (tx, rx) = channel();
        tx.send(1);
        assert!(rx.ready());
        assert_eq!(rx.recv(), 1);
        assert!(rx.try_recv().is_none());
        tx.send(1);
        assert!(rx.try_recv().is_some());
        assert!(rx.try_recv().is_none());
        let tx1 = tx;
        tx1.send(1);
    }

    #[test]
    fn recv_iter() {
        let (tx, rx) = channel();
        let counter = AtomicUsize::new(0);
        thread::scope(|s| {
            s.spawn(|| {
                for _ in rx.into_iter() {
                    counter.fetch_add(1, SeqCst);
                }
            });
            for i in 0..100 {
                tx.send(i);
            }
            drop(tx);
        });

        assert_eq!(counter.load(SeqCst), 100);
    }

    #[test]
    fn recv_ready() {
        let (tx, rx) = channel();
        assert!(!rx.ready());
        tx.send(1);
        assert!(rx.ready());
        assert_eq!(rx.channel.messages.load(SeqCst), 1);
        tx.send(1);
        assert_eq!(rx.channel.messages.load(SeqCst), 2);
        rx.recv();
        rx.recv();
        assert!(!rx.ready());
        assert!(rx.try_recv().is_none());
    }

    #[test]
    fn senders_count() {
        let (tx, _) = channel::<i32>();
        assert_eq!(tx.channel.senders.load(SeqCst), 1);
        let _tx1 = tx.clone();
        assert_eq!(tx.channel.senders.load(SeqCst), 2);
        drop(_tx1);
        assert_eq!(tx.channel.senders.load(SeqCst), 1);
        drop(tx);
    }
}
