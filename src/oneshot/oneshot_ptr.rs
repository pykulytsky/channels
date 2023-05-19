use std::sync::atomic::Ordering::*;
use std::thread::Thread;
use std::{cell::UnsafeCell, thread};
use std::{mem::MaybeUninit, sync::atomic::AtomicBool};

pub struct OneShot<T> {
    message: UnsafeCell<MaybeUninit<T>>,
    ready: AtomicBool,
}

unsafe impl<T> Sync for OneShot<T> where T: Send {}

impl<T> OneShot<T> {
    pub const fn new() -> Self {
        Self {
            message: UnsafeCell::new(MaybeUninit::uninit()),
            ready: AtomicBool::new(false),
        }
    }

    pub fn split(self) -> (Sender<T>, Receiver<T>) {
        let channel = Box::into_raw(Box::new(self));
        (
            Sender {
                channel,
                receiving_thread: thread::current(),
            },
            Receiver { channel },
        )
    }
}

impl<T> Drop for OneShot<T> {
    fn drop(&mut self) {
        if *self.ready.get_mut() {
            unsafe { self.message.get_mut().assume_init_drop() }
        }
    }
}

pub struct Sender<T> {
    channel: *mut OneShot<T>,
    receiving_thread: Thread,
}

// Safety: Sender<T> can not outlive Receiver<T>, which is not Send nor Sync.
unsafe impl<T> Send for Sender<T> {}

pub struct Receiver<T> {
    channel: *mut OneShot<T>,
}

impl<T> Sender<T> {
    fn channel(&self) -> &OneShot<T> {
        unsafe { &*self.channel }
    }
    pub fn send(self, message: T) {
        let channel = self.channel();
        unsafe { (*channel.message.get()).write(message) };
        channel.ready.store(true, Release);
        self.receiving_thread.unpark();
    }
}

impl<T> Receiver<T> {
    fn channel(&self) -> &OneShot<T> {
        unsafe { &*self.channel }
    }
    pub fn is_ready(&self) -> bool {
        self.channel().ready.load(Relaxed)
    }

    pub fn recv(self) -> T {
        while !self.channel().ready.swap(false, Acquire) {
            thread::park();
        }
        unsafe { (*self.channel().message.get()).assume_init_read() }
    }
}

impl<T> std::ops::Drop for Receiver<T> {
    fn drop(&mut self) {
        let _ = unsafe { Box::from_raw(self.channel) };
    }
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let channel = Box::into_raw(Box::new(OneShot::<T>::new()));

    (
        Sender {
            channel,
            receiving_thread: thread::current(),
        },
        Receiver { channel },
    )
}
