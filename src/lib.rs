//! Provides functionality for pausing, stopping, and resuming iterators, readers, and writers.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::{self, Read, Write};
use std::sync::{Arc, Condvar, Mutex, Weak};

/// A custom result type for halt.
pub type Result = std::result::Result<(), RemoteError>;

/// The remote error type.
#[derive(Copy, Clone, Debug)]
pub enum RemoteError {
    /// The `Halt` has been dropped.
    HaltIsDropped,
    /// Failed to take a lock on the mutex.
    FailedToLock,
}

impl Display for RemoteError {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            RemoteError::HaltIsDropped => f.write_str("the halt has been dropped"),
            RemoteError::FailedToLock => f.write_str("failed to take a lock on the mutex"),
        }
    }
}

impl Error for RemoteError {}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum State {
    Paused,
    Running,
    Stopped,
}

#[derive(Debug)]
struct Notify {
    state: Mutex<State>,
    condvar: Condvar,
}

/// A remote that allows for pausing, stopping, and resuming the `Halt` wrapper.
#[derive(Debug)]
pub struct Remote {
    notify: Weak<Notify>,
}

impl Remote {
    /// Pauses the iterator, causing the thread that runs the `Halt` to sleep until resumed or stopped.
    #[inline]
    pub fn pause(&self) -> Result {
        self.set(State::Paused)
    }

    /// Resumes the iterator, causing the `Halt` to run as normal.
    #[inline]
    pub fn resume(&self) -> Result {
        self.set_and_notify(State::Running)
    }

    /// Stops the iterator, causing the `Halt` to return `None` until resumed or paused.
    #[inline]
    pub fn stop(&self) -> Result {
        self.set_and_notify(State::Stopped)
    }

    #[inline]
    fn set(&self, state: State) -> Result {
        let notify = self.notify.upgrade().ok_or(RemoteError::HaltIsDropped)?;
        let mut guard = notify.state.lock().map_err(|_| RemoteError::FailedToLock)?;
        *guard = state;
        Ok(())
    }

    #[inline]
    fn set_and_notify(&self, state: State) -> Result {
        let notify = self.notify.upgrade().ok_or(RemoteError::HaltIsDropped)?;
        let mut guard = notify.state.lock().map_err(|_| RemoteError::FailedToLock)?;
        *guard = state;
        notify.condvar.notify_all();
        Ok(())
    }
}

/// A wrapper that makes it possible to pause, stop, and resume iterators, readers, and writers.
#[derive(Debug)]
pub struct Halt<T> {
    inner: T,
    notify: Arc<Notify>,
}

impl<T> Halt<T> {
    /// Returns a new wrapper around `T`.
    #[inline]
    pub fn new(inner: T) -> Halt<T> {
        Halt::from(inner)
    }

    /// Returns a remote that allows for pausing, stopping, and resuming the `T`.
    #[inline]
    pub fn remote(&self) -> Remote {
        Remote {
            notify: Arc::downgrade(&self.notify),
        }
    }

    #[inline]
    fn stopped(&self) -> bool {
        self.notify
            .state
            .lock()
            .map(|guard| *guard == State::Stopped)
            .unwrap_or(false)
    }

    #[inline]
    fn wait_if_paused(&self) -> Result {
        let mut guard = self
            .notify
            .state
            .lock()
            .map_err(|_| RemoteError::FailedToLock)?;
        while *guard == State::Paused {
            guard = self
                .notify
                .condvar
                .wait(guard)
                .map_err(|_| RemoteError::FailedToLock)?;
        }
        Ok(())
    }
}

impl<T> From<T> for Halt<T> {
    #[inline]
    fn from(inner: T) -> Self {
        Halt {
            inner,
            notify: Arc::new(Notify {
                state: Mutex::new(State::Running),
                condvar: Condvar::new(),
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
