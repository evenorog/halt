//! Provides functionality for pausing, stopping, and resuming threads.

#![doc(html_root_url = "https://docs.rs/halt")]
#![deny(missing_docs)]

mod worker;

pub use worker::Worker;

use std::sync::{Arc, Condvar, Mutex, Weak};
use Status::{Paused, Running, Stopped};

/// Helper for pausing, stopping, and resuming across threads.
#[derive(Debug, Default)]
pub struct Halt {
    state: Arc<State>,
}

impl Halt {
    /// Returns a new `Halt`.
    ///
    /// # Examples
    /// ```
    /// use halt::Halt;
    ///
    /// let _ = Halt::new();
    /// ```
    pub fn new() -> Self {
        Halt::default()
    }

    /// Returns a remote that allows for pausing, stopping, and resuming the `Halt`.
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

    /// Sleeps the current thread until resumed or stopped.
    fn wait_if_paused(&self) {
        let guard = self.state.status.lock().unwrap();
        let _guard = self
            .state
            .condvar
            .wait_while(guard, |status| *status == Paused)
            .unwrap();
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

/// A remote that allows for pausing, stopping, and resuming from another thread.
///
/// # Examples
/// ```
/// use halt::Halt;
///
/// let halt = Halt::new();
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
        let Some(state) = self.state.upgrade() else {
            return false;
        };

        let mut guard = state.status.lock().unwrap();
        let status = &mut *guard;
        let need_to_notify = *status == Paused && *status != new;
        *status = new;
        drop(guard);
        if need_to_notify {
            state.condvar.notify_all();
        }
        true
    }
}
