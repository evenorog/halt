//! Provides worker threads that can be paused, stopped, and resumed.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::thread::{self, JoinHandle, Thread};

use Signal::{Kill, Pause, Run, Stop};

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
        self.remote.set(Kill);
    }
}

impl Default for Worker {
    fn default() -> Self {
        Worker::new()
    }
}

impl Worker {
    /// Creates a new worker that is ready to run tasks.
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<Task>();
        let waiter = Waiter::default();
        let remote = waiter.remote();

        let join_handle = thread::spawn(move || {
            while let Ok(task) = receiver.recv() {
                let g = waiter.wait_while_paused();
                match *g {
                    Kill => return,
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
    pub fn run<F>(&self, task: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.sender
            .send(Box::new(task))
            .expect("unable to send the task");
    }

    /// Returns the thread on which the worker is running.
    pub fn thread(&self) -> &Thread {
        self.join_handle.thread()
    }

    /// Resumes the `Worker` from a paused or stopped state into a running state.
    pub fn resume(&self) -> bool {
        self.remote.set(Run)
    }

    /// Pauses the `Worker`, causing it to sleep until resumed.
    pub fn pause(&self) -> bool {
        self.remote.set(Pause)
    }

    /// Stops the `Worker`, causing it to skip tasks.
    pub fn stop(&self) -> bool {
        self.remote.set(Stop)
    }

    /// Returns `true` if running.
    pub fn is_running(&self) -> bool {
        self.remote.is(Run)
    }

    /// Returns `true` if paused.
    pub fn is_paused(&self) -> bool {
        self.remote.is(Pause)
    }

    /// Returns `true` if stopped.
    pub fn is_stopped(&self) -> bool {
        self.remote.is(Stop)
    }
}

/// Helper for pausing, stopping, and resuming across threads.
#[derive(Debug, Default)]
struct Waiter {
    state: Arc<State>,
}

impl Waiter {
    /// Returns a remote that allows for pausing, stopping, and resuming.
    fn remote(&self) -> Remote {
        Remote {
            state: Arc::downgrade(&self.state),
        }
    }

    /// Sleeps the current thread until resumed or stopped.
    fn wait_while_paused(&self) -> MutexGuard<'_, Signal> {
        let guard = self.state.signal.lock().unwrap();
        self.state
            .condvar
            .wait_while(guard, |status| *status == Pause)
            .unwrap()
    }
}

/// A remote that allows for pausing, stopping, and resuming from another thread.
#[derive(Debug)]
struct Remote {
    state: Weak<State>,
}

impl Remote {
    fn set(&self, signal: Signal) -> bool {
        self.state.upgrade().is_some_and(|state| state.set(signal))
    }

    fn is(&self, signal: Signal) -> bool {
        self.state.upgrade().is_some_and(|state| state.is(signal))
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
enum Signal {
    #[default]
    Run,
    Pause,
    Stop,
    Kill,
}

#[derive(Debug, Default)]
struct State {
    signal: Mutex<Signal>,
    condvar: Condvar,
}

impl State {
    fn set(&self, signal: Signal) -> bool {
        let Ok(mut guard) = self.signal.lock() else {
            return false;
        };

        *guard = signal;
        self.condvar.notify_all();
        true
    }

    fn is(&self, signal: Signal) -> bool {
        self.signal.lock().is_ok_and(|guard| *guard == signal)
    }
}
