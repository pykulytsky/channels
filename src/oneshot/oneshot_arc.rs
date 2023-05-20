use std::sync::Arc;
use std::thread::Thread;
use std::{cell::UnsafeCell, thread};
use std::{marker::PhantomData, sync::atomic::Ordering::*};
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
}

impl<T> Drop for OneShot<T> {
    fn drop(&mut self) {
        if *self.ready.get_mut() {
            unsafe { self.message.get_mut().assume_init_drop() }
        }
    }
}

pub struct Sender<T> {
    channel: Arc<OneShot<T>>,
    receiving_thread: Thread,
}

pub struct Receiver<T> {
    channel: Arc<OneShot<T>>,
    _no_send: PhantomData<*const ()>,
}

impl<T> Sender<T> {
    pub fn send(self, message: T) {
        unsafe { (*self.channel.message.get()).write(message) };
        self.channel.ready.store(true, Release);
        self.receiving_thread.unpark();
    }
}

impl<T> Receiver<T> {
    pub fn is_ready(&self) -> bool {
        self.channel.ready.load(Relaxed)
    }

    pub fn recv(self) -> T {
        while !self.channel.ready.swap(false, Acquire) {
            thread::park();
        }
        unsafe { (*self.channel.message.get()).assume_init_read() }
    }
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let channel = Arc::new(OneShot::<T>::new());

    (
        Sender {
            channel: channel.clone(),
            receiving_thread: thread::current(),
        },
        Receiver {
            channel,
            _no_send: PhantomData,
        },
    )
}
