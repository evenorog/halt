use crate::{Halt, Remote};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

type Task = Box<dyn FnOnce() + Send>;

/// A worker thread that can be paused and resumed.
#[derive(Debug)]
pub struct Worker {
    remote: Remote,
    sender: Sender<Task>,
}

impl Default for Worker {
    fn default() -> Self {
        Worker::new()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.remote.stop();
    }
}

impl Worker {
    /// Creates a new worker.
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<Task>();
        let halt = Halt::new(());
        let remote = halt.remote();

        // This threads runs the tasks detached.
        // We stop it by calling remote.stop().
        thread::spawn(move || {
            // FIXME: recv might block indefinitely.
            while let Ok(f) = receiver.recv() {
                halt.wait_if_paused();
                if halt.is_stopped() {
                    break;
                }

                f();
            }
        });

        Worker { remote, sender }
    }

    /// Run `f` on the worker thread.
    pub fn run<O>(&self, f: impl FnOnce() -> O + Send + 'static) -> Option<Receiver<O>>
    where
        O: Send + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel(1);

        let task = Box::new(move || {
            let output = f();
            sender.send(output).ok();
        });

        self.sender.send(task).map(|_| receiver).ok()
    }

    /// Returns a remote that allows for pausing, stopping, and resuming the worker.
    pub fn remote(&self) -> Remote {
        self.remote.clone()
    }
}
