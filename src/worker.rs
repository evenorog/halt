use crate::Status::{Paused, Running, Stopped};
use crate::{Halt, Remote};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, SendError, Sender};
use std::thread::{self, JoinHandle, Thread};

type Task = Box<dyn FnOnce() + Send>;

/// A worker thread that can be paused and resumed.
#[derive(Debug)]
pub struct Worker {
    remote: Remote,
    sender: Sender<Task>,
    join_handle: JoinHandle<()>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        // If the thread is paused when we drop the worker
        // it can never be resumed and the thread will never exit.
        // It is fine to let it be if it is running or stopped.
        self.remote.stop_if_paused();
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
            while let Ok(f) = receiver.recv() {
                halt.wait_if_paused();
                if halt.is_stopped() {
                    return;
                }

                f();
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

    /// Stops the `Worker`, causing it to exit.
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
