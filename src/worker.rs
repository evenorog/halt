use crate::{Halt, Remote};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::thread::JoinHandle;

type Task = Box<dyn FnOnce() + Send>;

/// A worker thread that can be paused and resumed.
#[derive(Debug)]
pub struct Worker {
    remote: Remote,
    sender: Sender<Task>,
    join_handle: Option<JoinHandle<()>>,
}

impl Default for Worker {
    /// Creates a new worker.
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel::<Task>();
        let halt = Halt::new(());
        let remote = halt.remote();

        let join = thread::spawn(move || {
            // FIXME: recv might block indefinitely.
            while let Ok(f) = receiver.recv() {
                halt.wait_if_paused();
                if halt.is_stopped() {
                    break;
                }

                f();
            }
        });

        Worker {
            remote,
            sender,
            join_handle: Some(join),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.remote.stop();
        if let Some(handle) = self.join_handle.take() {
            handle.join().ok();
        }
    }
}

impl Worker {
    /// Run `f` on the worker thread.
    pub fn run<O>(&self, f: impl FnOnce() -> O + Send + 'static) -> Receiver<O>
    where
        O: Send + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.sender
            .send(Box::new(move || {
                let output = f();
                sender.send(output).ok();
            }))
            .unwrap();
        receiver
    }

    /// Returns a remote that allows for pausing, stopping, and resuming the worker.
    pub fn remote(&self) -> Remote {
        self.remote.clone()
    }
}
