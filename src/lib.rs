//! Provides functionality for pausing, stopping, and resuming iterators, readers, and writers.
//!
//! # Examples
//!
//! Add this to `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! halt = "0.5"
//! ```
//!
//! And this to `main.rs`:
//!
//! ```no_run
//! use std::{io, thread, time::Duration};
//!
//! fn main() {
//!     let mut halt = halt::new(io::repeat(0));
//!     let remote = halt.remote();
//!     thread::spawn(move || io::copy(&mut halt, &mut io::sink()).unwrap());
//!     thread::sleep(Duration::from_secs(5));
//!     remote.pause();
//!     thread::sleep(Duration::from_secs(5));
//!     remote.resume();
//! }
//! ```

#![doc(html_root_url = "https://docs.rs/halt/latest")]
#![deny(
    bad_style,
    bare_trait_objects,
    missing_docs,
    unused_import_braces,
    unused_qualifications,
    unsafe_code,
    unstable_features
)]

use std::io::{self, Read, Write};
use std::sync::{Arc, Condvar, Mutex, Weak};
use Status::{Paused, Running, Stopped};

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
enum Status {
    Running,
    Paused,
    Stopped,
}

impl Default for Status {
    #[inline]
    fn default() -> Self {
        Running
    }
}

#[derive(Debug, Default)]
struct State {
    status: Mutex<Status>,
    condvar: Condvar,
}

impl State {
    #[inline]
    fn is_running(&self) -> bool {
        self.status
            .lock()
            .map(|status| *status == Running)
            .unwrap_or_default()
    }

    #[inline]
    fn is_paused(&self) -> bool {
        self.status
            .lock()
            .map(|status| *status == Paused)
            .unwrap_or_default()
    }

    #[inline]
    fn is_stopped(&self) -> bool {
        self.status
            .lock()
            .map(|status| *status == Stopped)
            .unwrap_or_default()
    }
}

/// A remote that allows for pausing, stopping, and resuming the `Halt` wrapper from another thread.
///
/// # Examples
/// ```
/// let halt = halt::new(0..10);
/// let remote = halt.remote();
/// ```
#[derive(Clone, Debug)]
pub struct Remote {
    state: Weak<State>,
}

impl Remote {
    /// Resumes the `Halt`, causing it to run as normal.
    ///
    /// # Panics
    /// Panics if the `Halt` has been dropped.
    #[inline]
    pub fn resume(&self) {
        self.set_and_notify(Running);
    }

    /// Pauses the `Halt`, causing the thread that runs it to sleep until resumed or stopped.
    ///
    /// # Panics
    /// Panics if the `Halt` has been dropped.
    #[inline]
    pub fn pause(&self) {
        self.set_and_notify(Paused);
    }

    /// Stops the `Halt`, causing it to behave as done until resumed or paused.
    ///
    /// When `Halt` is used as an iterator, the iterator will return `None`.
    /// When used as a reader or writer, it will return `Ok(0)`.
    ///
    /// # Panics
    /// Panics if the `Halt` has been dropped.
    #[inline]
    pub fn stop(&self) {
        self.set_and_notify(Stopped);
    }

    /// Returns `true` if the `Remote` is valid, i.e. the `Halt` has not been dropped.
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.state.upgrade().is_some()
    }

    /// Returns `true` if running.
    #[inline]
    pub fn is_running(&self) -> bool {
        self.state
            .upgrade()
            .map(|state| state.is_running())
            .unwrap_or_default()
    }

    /// Returns `true` if paused.
    #[inline]
    pub fn is_paused(&self) -> bool {
        self.state
            .upgrade()
            .map(|state| state.is_paused())
            .unwrap_or_default()
    }

    /// Returns `true` if stopped.
    #[inline]
    pub fn is_stopped(&self) -> bool {
        self.state
            .upgrade()
            .map(|state| state.is_stopped())
            .unwrap_or_default()
    }

    #[inline]
    fn set_and_notify(&self, new: Status) {
        let state = self.state.upgrade().expect("invalid remote");
        let mut guard = state.status.lock().unwrap();
        let status = &mut *guard;
        let need_to_notify = *status == Paused && *status != new;
        *status = new;
        drop(guard);
        if need_to_notify {
            state.condvar.notify_all();
        }
    }
}

/// Returns a new wrapper around `T`.
#[inline]
pub fn new<T>(inner: T) -> Halt<T> {
    Halt::from(inner)
}

/// A wrapper that makes it possible to pause, stop, and resume iterators, readers, and writers.
///
/// # Examples
/// ```
/// halt::new(0..10);
/// ```
#[derive(Debug, Default)]
pub struct Halt<T> {
    inner: T,
    state: Arc<State>,
}

impl<T> Halt<T> {
    /// Returns a remote that allows for pausing, stopping, and resuming the `T`.
    #[inline]
    pub fn remote(&self) -> Remote {
        Remote {
            state: Arc::downgrade(&self.state),
        }
    }

    /// Returns `true` if running.
    #[inline]
    pub fn is_running(&self) -> bool {
        self.state.is_running()
    }

    /// Returns `true` if paused.
    #[inline]
    pub fn is_paused(&self) -> bool {
        self.state.is_paused()
    }

    /// Returns `true` if stopped.
    #[inline]
    pub fn is_stopped(&self) -> bool {
        self.state.is_stopped()
    }

    /// Returns a reference to the inner `T`.
    #[inline]
    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    /// Returns a mutable reference to the inner `T`.
    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Returns the inner `T`.
    #[inline]
    pub fn into_inner(self) -> T {
        self.inner
    }

    #[inline]
    fn wait_if_paused(&self) {
        let mut guard = self.state.status.lock().unwrap();
        while *guard == Paused {
            guard = self.state.condvar.wait(guard).unwrap();
        }
    }
}

impl<T> From<T> for Halt<T> {
    #[inline]
    fn from(inner: T) -> Self {
        Halt {
            inner,
            state: Arc::new(State::default()),
        }
    }
}

impl<I: Iterator> Iterator for Halt<I> {
    type Item = I::Item;

    #[inline]
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
    #[inline]
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
    #[inline]
    fn extend<T: IntoIterator<Item = A>>(&mut self, iter: T) {
        self.inner.extend(iter);
    }
}

impl<R: Read> Read for Halt<R> {
    #[inline]
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
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.wait_if_paused();
        if self.is_stopped() {
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
