//! Provides worker threads that can be paused, stopped, and resumed.

#![doc(html_root_url = "https://docs.rs/halt")]
#![deny(missing_docs)]

use std::sync::mpsc::{self, Receiver, SendError, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::thread::{self, JoinHandle, Thread};

use Action::{Exit, Pause, Run, Stop};

type Task = Box<dyn FnOnce() + Send>;

/// A worker thread that can be paused, stopped, and resumed.
#[derive(Debug)]
pub struct Worker {
    remote: Remote,
    sender: Sender<Task>,
    join_handle: JoinHandle<()>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Exits the worker thread.
        self.remote.exit();
    }
}

impl Default for Worker {
    fn default() -> Self {
        Worker::new()
    }
}

impl Worker {
    /// Creates a new worker.
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<Task>();
        let waiter = Waiter::default();
        let remote = waiter.remote();

        let join_handle = thread::spawn(move || {
            while let Ok(task) = receiver.recv() {
                let g = waiter.wait_while_paused();
                match *g {
                    Exit => return,
                    Stop => continue,
                    Run | Pause => drop(g),
                }

                task()
            }
        });

        Worker {
            remote,
            sender,
            join_handle,
        }
    }

    /// Run `f` on the worker thread.
    pub fn run<T>(
        &self,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> Result<Receiver<T>, SendError<Task>>
    where
        T: Send + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel(1);

        let task = Box::new(move || {
            let x = f();
            sender.send(x).ok();
        });

        self.sender.send(task).map(|_| receiver)
    }

    /// Returns the thread on which the worker is running.
    pub fn thread(&self) -> &Thread {
        self.join_handle.thread()
    }

    /// Resumes the `Worker` from a paused state.
    pub fn resume(&self) -> bool {
        self.remote.resume()
    }

    /// Pauses the `Worker`, causing it to sleep until resumed.
    pub fn pause(&self) -> bool {
        self.remote.pause()
    }

    /// Stops the `Worker`, causing it to skip tasks.
    pub fn stop(&self) -> bool {
        self.remote.stop()
    }

    /// Returns `true` if running.
    pub fn is_running(&self) -> bool {
        self.remote.is_running()
    }

    /// Returns `true` if paused.
    pub fn is_paused(&self) -> bool {
        self.remote.is_paused()
    }

    /// Returns `true` if stopped.
    pub fn is_stopped(&self) -> bool {
        self.remote.is_stopped()
    }
}

/// Helper for pausing, stopping, and resuming across threads.
#[derive(Debug, Default)]
struct Waiter {
    state: Arc<State>,
}

impl Waiter {
    /// Returns a remote that allows for pausing, stopping, and resuming the `Halt`.
    fn remote(&self) -> Remote {
        Remote {
            state: Arc::downgrade(&self.state),
        }
    }

    /// Sleeps the current thread until resumed or stopped.
    fn wait_while_paused(&self) -> MutexGuard<'_, Action> {
        let guard = self.state.action.lock().unwrap();
        self.state
            .condvar
            .wait_while(guard, |status| *status == Pause)
            .unwrap()
    }
}

/// A remote that allows for pausing, stopping, and resuming from another thread.
///
/// # Examples
/// ```
/// use halt::Waiter;
///
/// let halt = Waiter::new();
/// let remote = halt.remote();
/// ```
#[derive(Clone, Debug)]
pub struct Remote {
    state: Weak<State>,
}

impl Remote {
    /// Resumes the `Halt`.
    ///
    /// Returns `true` if the remote [`is_valid`](Remote::is_valid).
    pub fn resume(&self) -> bool {
        self.state.upgrade().is_some_and(|state| state.set(Run))
    }

    /// Pauses the `Halt`, causing the thread to sleep.
    ///
    /// Returns `true` if the remote [`is_valid`](Remote::is_valid).
    pub fn pause(&self) -> bool {
        self.state.upgrade().is_some_and(|state| state.set(Pause))
    }

    /// Stops the `Halt`, causing it to behave as done until resumed or paused.
    ///
    /// Returns `true` if the remote [`is_valid`](Remote::is_valid).
    pub fn stop(&self) -> bool {
        self.state.upgrade().is_some_and(|state| state.set(Stop))
    }

    pub(crate) fn exit(&self) -> bool {
        self.state.upgrade().is_some_and(|state| state.set(Exit))
    }

    /// Returns `true` if the remote is valid, i.e. the `thread` has not been dropped.
    pub fn is_valid(&self) -> bool {
        self.state.strong_count() != 0
    }

    /// Returns `true` if running.
    pub fn is_running(&self) -> bool {
        self.state.upgrade().is_some_and(|state| state.is(Run))
    }

    /// Returns `true` if paused.
    pub fn is_paused(&self) -> bool {
        self.state.upgrade().is_some_and(|state| state.is(Pause))
    }

    /// Returns `true` if stopped.
    pub fn is_stopped(&self) -> bool {
        self.state.upgrade().is_some_and(|state| state.is(Stop))
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
enum Action {
    #[default]
    Run,
    Pause,
    Stop,
    Exit,
}

#[derive(Debug, Default)]
struct State {
    action: Mutex<Action>,
    condvar: Condvar,
}

impl State {
    fn set(&self, new: Action) -> bool {
        let Ok(mut guard) = self.action.lock() else {
            return false;
        };

        *guard = new;
        self.condvar.notify_one();
        true
    }

    fn is(&self, action: Action) -> bool {
        self.action.lock().is_ok_and(|guard| *guard == action)
    }
}
