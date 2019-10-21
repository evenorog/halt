//! Provides functionality for pausing, stopping, and resuming iterators, readers, and writers.
//!
//! # Examples
//!
//! Add this to `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! halt = "0.1"
//! ```
//!
//! And this to `main.rs`:
//!
//! ```no_run
//! use halt::Halt;
//! use std::io;
//! use std::thread;
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

use std::error;
use std::fmt::{self, Display, Formatter};
use std::io::{self, Read, Write};
use std::sync::{Arc, Condvar, Mutex, Weak};

/// A specialized result type for halt.
pub type Result = std::result::Result<(), Error>;

/// The error type used in halt.
#[derive(Copy, Clone, Debug)]
pub enum Error {
    /// The `Halt` wrapper has been dropped.
    HaltIsDropped,
    /// Failed to take a lock on the mutex.
    FailedToLock,
}

impl Display for Error {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Error::HaltIsDropped => f.write_str("the halt wrapper has been dropped"),
            Error::FailedToLock => f.write_str("failed to take a lock on the mutex"),
        }
    }
}

impl error::Error for Error {}

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

impl Default for Notify {
    #[inline]
    fn default() -> Self {
        Notify {
            state: Mutex::new(State::Running),
            condvar: Condvar::new(),
        }
    }
}

/// A remote that allows for pausing, stopping, and resuming the `Halt` wrapper.
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
    /// Pauses the iterator, causing the thread that runs the `Halt` wrapper to sleep until resumed or stopped.
    #[inline]
    pub fn pause(&self) -> Result {
        self.set(State::Paused)
    }

    /// Resumes the iterator, causing the `Halt` wrapper to run as normal.
    #[inline]
    pub fn resume(&self) -> Result {
        self.set_and_notify(State::Running)
    }

    /// Stops the iterator, causing the `Halt` to behave as done until resumed or paused.
    ///
    /// When `Halt` is used as an iterator, the iterator will continuously return `None`.
    /// When used as a reader or writer, it will continuously return `Ok(0)`.
    #[inline]
    pub fn stop(&self) -> Result {
        self.set_and_notify(State::Stopped)
    }

    #[inline]
    fn set(&self, state: State) -> Result {
        let notify = self.notify.upgrade().ok_or(Error::HaltIsDropped)?;
        let mut guard = notify.state.lock().map_err(|_| Error::FailedToLock)?;
        *guard = state;
        Ok(())
    }

    #[inline]
    fn set_and_notify(&self, state: State) -> Result {
        let notify = self.notify.upgrade().ok_or(Error::HaltIsDropped)?;
        let mut guard = notify.state.lock().map_err(|_| Error::FailedToLock)?;
        *guard = state;
        notify.condvar.notify_all();
        Ok(())
    }
}

/// A wrapper that makes it possible to pause, stop, and resume iterators, readers, and writers.
///
/// # Examples
/// ```
/// # use halt::Halt;
/// let halt = Halt::new(0..10);
/// ```
#[derive(Debug)]
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
    fn stopped(&self) -> bool {
        self.notify
            .state
            .lock()
            .map(|guard| *guard == State::Stopped)
            .unwrap_or(false)
    }

    #[inline]
    fn wait_if_paused(&self) -> Result {
        let mut guard = self.notify.state.lock().map_err(|_| Error::FailedToLock)?;
        while *guard == State::Paused {
            guard = self
                .notify
                .condvar
                .wait(guard)
                .map_err(|_| Error::FailedToLock)?;
        }
        Ok(())
    }
}

impl<T> From<T> for Halt<T> {
    #[inline]
    fn from(inner: T) -> Self {
        Halt::new(inner)
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
