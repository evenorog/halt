//! Provides functionality for pausing and resuming iterators, readers, and writers.

use std::error;
use std::fmt::{self, Display, Formatter};
use std::io::{self, Read, Write};
use std::sync::{Arc, Condvar, Mutex, Weak};

/// A custom result type for halt.
pub type Result = std::result::Result<(), Error>;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum State {
    Paused,
    Running,
    Stopped,
}

/// The error type.
#[derive(Copy, Clone, Debug)]
pub enum Error {
    /// The receiver has been dropped.
    NoReceiver,
    /// Failed to take a lock on the receiver.
    FailedToLock,
}

impl Display for Error {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Error::NoReceiver => f.write_str("the receiver has been dropped"),
            Error::FailedToLock => f.write_str("failed to take a lock on the receiver"),
        }
    }
}

impl error::Error for Error {}

/// A controller that allows for pausing, stopping, and resuming the `Halt` wrapper.
#[derive(Debug)]
pub struct Controller {
    receiver: Weak<Receiver>,
}

impl Controller {
    /// Pauses the iterator, causes the thread that runs the iterator to sleep until resumed or stopped.
    #[inline]
    pub fn pause(&self) -> Result {
        self.set(State::Paused)
    }

    /// Resumes the iterator, causes the iterator to run as normal.
    #[inline]
    pub fn resume(&self) -> Result {
        self.set_and_notify(State::Running)
    }

    /// Stops the iterator, causes the iterator to return `None` until resumed or paused.
    #[inline]
    pub fn stop(&self) -> Result {
        self.set_and_notify(State::Stopped)
    }

    #[inline]
    fn set(&self, state: State) -> Result {
        match self.receiver.upgrade() {
            Some(receiver) => {
                let mut guard = receiver.state.lock().map_err(|_| Error::FailedToLock)?;
                *guard = state;
                Ok(())
            }
            None => Err(Error::NoReceiver),
        }
    }

    #[inline]
    fn set_and_notify(&self, state: State) -> Result {
        match self.receiver.upgrade() {
            Some(receiver) => {
                let mut guard = receiver.state.lock().map_err(|_| Error::FailedToLock)?;
                *guard = state;
                receiver.notify.notify_all();
                Ok(())
            }
            None => Err(Error::NoReceiver),
        }
    }
}

#[derive(Debug)]
struct Receiver {
    state: Mutex<State>,
    notify: Condvar,
}

/// A wrapper that makes it possible to pause, stop, and resume iterators, readers, and writers.
#[derive(Debug)]
pub struct Halt<T> {
    inner: T,
    receiver: Arc<Receiver>,
}

impl<T> Halt<T> {
    /// Returns a new wrapper around `T`.
    #[inline]
    pub fn new(inner: T) -> Halt<T> {
        Halt::from(inner)
    }

    /// Returns a controller that allows for pausing, stopping, and resuming the `T`.
    #[inline]
    pub fn controller(&self) -> Controller {
        Controller {
            receiver: Arc::downgrade(&self.receiver),
        }
    }

    #[inline]
    fn stopped(&self) -> bool {
        self.receiver
            .state
            .lock()
            .map(|guard| *guard == State::Stopped)
            .unwrap_or(false)
    }

    #[inline]
    fn wait_if_paused(&self) -> Result {
        let mut guard = self
            .receiver
            .state
            .lock()
            .map_err(|_| Error::FailedToLock)?;
        while *guard == State::Paused {
            guard = self
                .receiver
                .notify
                .wait(guard)
                .map_err(|_| Error::FailedToLock)?;
        }
        Ok(())
    }
}

impl<T> From<T> for Halt<T> {
    #[inline]
    fn from(inner: T) -> Self {
        Halt {
            inner,
            receiver: Arc::new(Receiver {
                state: Mutex::new(State::Running),
                notify: Condvar::new(),
            }),
        }
    }
}

impl<I: Iterator> Iterator for Halt<I> {
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let _ = self.wait_if_paused();
        if self.stopped() {
            None
        } else {
            self.inner.next()
        }
    }
}

impl<I: DoubleEndedIterator> DoubleEndedIterator for Halt<I> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        let _ = self.wait_if_paused();
        if self.stopped() {
            None
        } else {
            self.inner.next_back()
        }
    }
}

impl<I: Extend<A>, A> Extend<A> for Halt<I> {
    #[inline]
    fn extend<T: IntoIterator<Item = A>>(&mut self, iter: T) {
        self.inner.extend(iter);
    }
}

impl<R: Read> Read for Halt<R> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let _ = self.wait_if_paused();
        if self.stopped() {
            Ok(0)
        } else {
            self.inner.read(buf)
        }
    }
}

impl<W: Write> Write for Halt<W> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let _ = self.wait_if_paused();
        if self.stopped() {
            Ok(0)
        } else {
            self.inner.write(buf)
        }
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
