//! Provides functionality for pausing, stopping, and resuming iterators, readers, and writers.
//!
//! # Examples
//!
//! Add this to `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! halt = "0.3"
//! ```
//!
//! And this to `main.rs`:
//!
//! ```no_run
//! use halt::Halt;
//! use std::{io, thread};
//!
//! fn main() {
//!     // Wrap a reader in the halt structure.
//!     let mut halt = Halt::new(io::repeat(0));
//!     // Get a remote to the reader.
//!     let remote = halt.remote();
//!     // Copy forever into a sink, in a separate thread.
//!     thread::spawn(move || io::copy(&mut halt, &mut io::sink()).unwrap());
//!     // The remote can now be used to either pause, stop, or resume the reader from the main thread.
//!     remote.pause();
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

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
enum State {
    Running,
    Paused,
    Stopped,
}

impl Default for State {
    #[inline]
    fn default() -> Self {
        State::Running
    }
}

#[derive(Debug, Default)]
struct Notify {
    state: Mutex<State>,
    condvar: Condvar,
}

impl Notify {
    #[inline]
    fn is_paused(&self) -> bool {
        self.state
            .lock()
            .map(|state| *state == State::Paused)
            .unwrap_or(false)
    }

    #[inline]
    fn is_running(&self) -> bool {
        self.state
            .lock()
            .map(|state| *state == State::Running)
            .unwrap_or(false)
    }

    #[inline]
    fn is_stopped(&self) -> bool {
        self.state
            .lock()
            .map(|state| *state == State::Stopped)
            .unwrap_or(false)
    }
}

/// A remote that allows for pausing, stopping, and resuming the `Halt` wrapper from another thread.
///
/// # Examples
/// ```
/// # use halt::Halt;
/// let halt = Halt::new(0..10);
/// let remote = halt.remote();
/// ```
#[derive(Clone, Debug)]
pub struct Remote {
    notify: Weak<Notify>,
}

impl Remote {
    /// Pauses the `Halt`, causing the thread that runs it to sleep until resumed or stopped.
    ///
    /// # Panics
    /// Panics if the `Halt` has been dropped.
    #[inline]
    pub fn pause(&self) {
        self.set_and_notify(State::Paused);
    }

    /// Resumes the `Halt`, causing it to run as normal.
    ///
    /// # Panics
    /// Panics if the `Halt` has been dropped.
    #[inline]
    pub fn resume(&self) {
        self.set_and_notify(State::Running);
    }

    /// Stops the `Halt`, causing it to behave as done until resumed or paused.
    ///
    /// When `Halt` is used as an iterator, the iterator will continuously return `None`.
    /// When used as a reader or writer, it will continuously return `Ok(0)`.
    ///
    /// # Panics
    /// Panics if the `Halt` has been dropped.
    #[inline]
    pub fn stop(&self) {
        self.set_and_notify(State::Stopped);
    }

    /// Returns `true` if paused.
    #[inline]
    pub fn is_paused(&self) -> bool {
        self.notify
            .upgrade()
            .map(|notify| notify.is_paused())
            .unwrap_or(false)
    }

    /// Returns `true` if running.
    #[inline]
    pub fn is_running(&self) -> bool {
        self.notify
            .upgrade()
            .map(|notify| notify.is_running())
            .unwrap_or(false)
    }

    /// Returns `true` if stopped.
    #[inline]
    pub fn is_stopped(&self) -> bool {
        self.notify
            .upgrade()
            .map(|notify| notify.is_stopped())
            .unwrap_or(false)
    }

    /// Returns `true` if the `Remote` is valid, i.e. the `Halt` has not been dropped.
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.notify.upgrade().is_some()
    }

    #[inline]
    fn set_and_notify(&self, new: State) {
        let notify = self.notify.upgrade().expect("invalid remote");
        let mut guard = notify.state.lock().unwrap();
        let state = &mut *guard;
        let need_to_notify = *state == State::Paused && *state != new;
        *state = new;
        drop(guard);
        if need_to_notify {
            notify.condvar.notify_all();
        }
    }
}

/// A wrapper that makes it possible to pause, stop, and resume iterators, readers, and writers.
///
/// # Examples
/// ```
/// # use halt::Halt;
/// let halt = Halt::new(0..10);
/// ```
#[derive(Debug, Default)]
pub struct Halt<T> {
    inner: T,
    notify: Arc<Notify>,
}

impl<T> Halt<T> {
    /// Returns a new wrapper around `T`.
    #[inline]
    pub fn new(inner: T) -> Halt<T> {
        Halt {
            inner,
            notify: Arc::new(Notify::default()),
        }
    }

    /// Returns a remote that allows for pausing, stopping, and resuming the `T`.
    #[inline]
    pub fn remote(&self) -> Remote {
        Remote {
            notify: Arc::downgrade(&self.notify),
        }
    }

    /// Returns `true` if paused.
    #[inline]
    pub fn is_paused(&self) -> bool {
        self.notify.is_paused()
    }

    /// Returns `true` if running.
    #[inline]
    pub fn is_running(&self) -> bool {
        self.notify.is_running()
    }

    /// Returns `true` if stopped.
    #[inline]
    pub fn is_stopped(&self) -> bool {
        self.notify.is_stopped()
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
        let mut guard = self.notify.state.lock().unwrap();
        while *guard == State::Paused {
            guard = self.notify.condvar.wait(guard).unwrap();
        }
    }
}

impl<T> From<T> for Halt<T> {
    #[inline]
    fn from(inner: T) -> Self {
        Halt::new(inner)
    }
}

impl<T> AsRef<T> for Halt<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        self.get_ref()
    }
}

impl<T> AsMut<T> for Halt<T> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        self.get_mut()
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
