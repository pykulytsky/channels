use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use crate::utils::{
    queue::Queue,
    wait::{wait, wake_one},
};
use crossbeam_epoch::pin;

pub struct Channel<T> {
    queue: Queue<T>,
    messages: AtomicUsize,
}

impl<T> Channel<T> {
    pub fn new() -> Self {
        Self {
            queue: Queue::new(),
            messages: AtomicUsize::new(0),
        }
    }
}

impl<T> Default for Channel<T> {
    fn default() -> Self {
        Self::new()
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
        Self {
            channel: self.channel.clone(),
        }
    }
}

pub struct Receiver<T> {
    channel: Arc<Channel<T>>,
}

#[derive(Debug)]
pub struct RecvError;

impl<T> Receiver<T> {
    /// Returns the senders remaining of this [`Channel<T>`].
    /// Since there is always 1 `Receiver<T>` holding the clone of `Channel<T>`, we substract 1.
    #[inline]
    fn senders_remaining(&self) -> usize {
        Arc::strong_count(&self.channel) - 1
    }

    #[inline]
    fn messages_remaining(&self) -> usize {
        self.channel.messages.load(Ordering::Acquire)
    }

    #[inline]
    pub fn ready(&self) -> bool {
        self.messages_remaining() > 0
    }

    #[inline]
    pub fn recv(&self) -> Result<T, RecvError> {
        let guard = &pin();
        if self.messages_remaining() < 1 && self.senders_remaining() < 1 {
            return Err(RecvError);
        }
        loop {
            let mut messages = self.channel.messages.load(Ordering::Acquire);
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

        self.channel.queue.try_pop(guard).ok_or_else(|| RecvError)
    }

    #[inline]
    pub fn try_recv(&self) -> Option<T> {
        if self.senders_remaining() < 1 && self.messages_remaining() < 1 {
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
        self.rx.recv().ok()
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
        self.rx.recv().ok()
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
    let channel = Arc::new(Channel::<T>::default());

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
        assert_eq!(rx.recv().unwrap(), 1);
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
                for i in rx.into_iter() {
                    println!("{i}");
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
        let _ = rx.recv();
        let _ = rx.recv();
        assert!(!rx.ready());
        assert!(rx.try_recv().is_none());
    }

    #[test]
    fn senders_count() {
        let (tx, rx) = channel::<i32>();
        assert_eq!(rx.senders_remaining(), 1);
        let _tx1 = tx.clone();
        assert_eq!(rx.senders_remaining(), 2);
        drop(_tx1);
        assert_eq!(rx.senders_remaining(), 1);
        drop(tx);
        assert_eq!(rx.senders_remaining(), 0);
    }
}
