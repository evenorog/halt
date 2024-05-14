use std::sync::mpsc;
use std::sync::mpsc::{Receiver, SendError, Sender};
use std::thread::{self, JoinHandle, Thread};

use crate::{Halt, Remote, Status::*};

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
        self.remote.done();
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
        let halt = Halt::new();
        let remote = halt.remote();

        let join_handle = thread::spawn(move || {
            while let Ok(task) = receiver.recv() {
                // We sleep the thread when paused.
                let g = halt.wait_while_paused();

                // We skip tasks if stopped.
                if *g == Stopped {
                    continue;
                }

                // We exit and terminate when done.
                if *g == Done {
                    return;
                }

                task();
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
