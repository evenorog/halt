//! Provides functionality for pausing, stopping, and resuming iterators, readers, and writers.
//!
//! ```no_run
//! use std::{io, thread, time::Duration};
//! use halt::Halt;
//!
//! let mut halt = Halt::new(io::repeat(0));
//! let remote = halt.remote();
//! thread::spawn(move || io::copy(&mut halt, &mut io::sink()).unwrap());
//!
//! thread::sleep(Duration::from_secs(5));
//! remote.pause();
//! thread::sleep(Duration::from_secs(5));
//! remote.resume();
//! thread::sleep(Duration::from_secs(5));
//! ```

#![doc(html_root_url = "https://docs.rs/halt")]
#![deny(missing_docs)]

mod worker;

pub use worker::Worker;

use std::io::{self, Read, Write};
use std::sync::{Arc, Condvar, Mutex, Weak};
use Status::{Paused, Running, Stopped};

/// A wrapper that makes it possible to pause, stop, and resume iterators, readers, and writers.
#[derive(Debug, Default)]
pub struct Halt<T> {
    inner: T,
    state: Arc<State>,
}

impl<T> Halt<T> {
    /// Returns a new `Halt` wrapper around `T`.
    ///
    /// # Examples
    /// ```
    /// use halt::Halt;
    ///
    /// let _ = Halt::new(0..10);
    /// ```
    pub fn new(inner: T) -> Self {
        Halt::from(inner)
    }

    /// Returns a remote that allows for pausing, stopping, and resuming the `T`.
    pub fn remote(&self) -> Remote {
        Remote {
            state: Arc::downgrade(&self.state),
        }
    }

    /// Returns `true` if running.
    pub fn is_running(&self) -> bool {
        self.state.is_running()
    }

    /// Returns `true` if paused.
    pub fn is_paused(&self) -> bool {
        self.state.is_paused()
    }

    /// Returns `true` if stopped.
    pub fn is_stopped(&self) -> bool {
        self.state.is_stopped()
    }

    /// Returns a reference to the inner `T`.
    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    /// Returns a mutable reference to the inner `T`.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Returns the inner `T`.
    pub fn into_inner(self) -> T {
        self.inner
    }

    fn wait_if_paused(&self) {
        let guard = self.state.status.lock().unwrap();
        let _guard = self
            .state
            .condvar
            .wait_while(guard, |status| *status == Paused)
            .unwrap();
    }
}

impl<T> From<T> for Halt<T> {
    fn from(inner: T) -> Self {
        Halt {
            inner,
            state: Arc::default(),
        }
    }
}

impl<I: Iterator> Iterator for Halt<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.wait_if_paused();
        if self.is_stopped() {
            None
        } else {
            self.inner.next()
        }
    }
}

impl<I: DoubleEndedIterator> DoubleEndedIterator for Halt<I> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.wait_if_paused();
        if self.is_stopped() {
            None
        } else {
            self.inner.next_back()
        }
    }
}

impl<A, I: Extend<A>> Extend<A> for Halt<I> {
    fn extend<T: IntoIterator<Item = A>>(&mut self, iter: T) {
        self.inner.extend(iter);
    }
}

impl<R: Read> Read for Halt<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.wait_if_paused();
        if self.is_stopped() {
            Ok(0)
        } else {
            self.inner.read(buf)
        }
    }
}

impl<W: Write> Write for Halt<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.wait_if_paused();
        if self.is_stopped() {
            Ok(0)
        } else {
            self.inner.write(buf)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[derive(Copy, Clone, Debug, Default, Hash, Eq, PartialEq, Ord, PartialOrd)]
enum Status {
    #[default]
    Running,
    Paused,
    Stopped,
}

#[derive(Debug, Default)]
struct State {
    status: Mutex<Status>,
    condvar: Condvar,
}

impl State {
    fn is_running(&self) -> bool {
        self.status
            .lock()
            .map_or(false, |status| *status == Running)
    }

    fn is_paused(&self) -> bool {
        self.status.lock().map_or(false, |status| *status == Paused)
    }

    fn is_stopped(&self) -> bool {
        self.status
            .lock()
            .map_or(false, |status| *status == Stopped)
    }
}

/// A remote that allows for pausing, stopping, and resuming the `Halt` wrapper from another thread.
///
/// # Examples
/// ```
/// use halt::Halt;
///
/// let halt = Halt::new(0..10);
/// let remote = halt.remote();
/// ```
#[derive(Clone, Debug)]
pub struct Remote {
    state: Weak<State>,
}

impl Remote {
    /// Resumes the `Halt`, causing it to run as normal.
    ///
    /// Returns `true` if the remote [`is_valid`](Remote::is_valid).
    pub fn resume(&self) -> bool {
        self.set_and_notify(Running)
    }

    /// Pauses the `Halt`, causing the thread that runs it to sleep until resumed or stopped.
    ///
    /// Returns `true` if the remote [`is_valid`](Remote::is_valid).
    pub fn pause(&self) -> bool {
        self.set_and_notify(Paused)
    }

    /// Stops the `Halt`, causing it to behave as done until resumed or paused.
    ///
    /// When `Halt` is used as an iterator, the iterator will return `None`.
    /// When used as a reader or writer, it will return `Ok(0)`.
    ///
    /// Returns `true` if the remote [`is_valid`](Remote::is_valid).
    pub fn stop(&self) -> bool {
        self.set_and_notify(Stopped)
    }

    /// Returns `true` if the remote is valid, i.e. the `Halt` has not been dropped.
    pub fn is_valid(&self) -> bool {
        self.state.strong_count() != 0
    }

    /// Returns `true` if running.
    pub fn is_running(&self) -> bool {
        self.state
            .upgrade()
            .map_or(false, |state| state.is_running())
    }

    /// Returns `true` if paused.
    pub fn is_paused(&self) -> bool {
        self.state
            .upgrade()
            .map_or(false, |state| state.is_paused())
    }

    /// Returns `true` if stopped.
    pub fn is_stopped(&self) -> bool {
        self.state
            .upgrade()
            .map_or(false, |state| state.is_stopped())
    }

    fn set_and_notify(&self, new: Status) -> bool {
        self.state.upgrade().map_or(false, |state| {
            let mut guard = state.status.lock().unwrap();
            let status = &mut *guard;
            let need_to_notify = *status == Paused && *status != new;
            *status = new;
            drop(guard);
            if need_to_notify {
                state.condvar.notify_all();
            }
            true
        })
    }
}
