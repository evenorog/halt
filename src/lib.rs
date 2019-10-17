use std::io::{self, Read, Write};
use std::sync::{Arc, Condvar, Mutex, Weak};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum State {
    Paused,
    Running,
    Stopped,
}

pub type Result = std::result::Result<(), Error>;

#[derive(Copy, Clone, Debug)]
pub enum Error {
    NoReceiver,
    FailedToLock,
}

#[derive(Debug)]
pub struct Controller {
    receiver: Weak<Receiver>,
}

impl Controller {
    #[inline]
    pub fn pause(&self) -> Result {
        self.set(State::Paused)
    }

    #[inline]
    pub fn resume(&self) -> Result {
        self.set_and_notify(State::Running)
    }

    #[inline]
    pub fn stop(&self) -> Result {
        self.set_and_notify(State::Stopped)
    }

    #[inline]
    fn set(&self, state: State) -> Result {
        match self.receiver.upgrade() {
            Some(receiver) => {
                let mut guard = receiver.state.lock().map_err(|_| Error::FailedToLock)?;
                *guard = state;
                Ok(())
            }
            None => Err(Error::NoReceiver),
        }
    }

    #[inline]
    fn set_and_notify(&self, state: State) -> Result {
        match self.receiver.upgrade() {
            Some(receiver) => {
                let mut guard = receiver.state.lock().map_err(|_| Error::FailedToLock)?;
                *guard = state;
                receiver.notify.notify_all();
                Ok(())
            }
            None => Err(Error::NoReceiver),
        }
    }
}

#[derive(Debug)]
struct Receiver {
    state: Mutex<State>,
    notify: Condvar,
}

#[derive(Debug)]
pub struct Halt<T> {
    inner: T,
    receiver: Arc<Receiver>,
}

impl<T> Halt<T> {
    #[inline]
    pub fn new(inner: impl Into<T>) -> Halt<T> {
        Halt::from(inner.into())
    }

    #[inline]
    pub fn controller(&self) -> Controller {
        Controller {
            receiver: Arc::downgrade(&self.receiver),
        }
    }

    #[inline]
    fn stopped(&self) -> bool {
        self.receiver
            .state
            .lock()
            .map(|guard| *guard == State::Stopped)
            .unwrap_or(false)
    }

    #[inline]
    fn wait_if_paused(&self) -> Result {
        let mut guard = self
            .receiver
            .state
            .lock()
            .map_err(|_| Error::FailedToLock)?;
        while *guard == State::Paused {
            guard = self
                .receiver
                .notify
                .wait(guard)
                .map_err(|_| Error::FailedToLock)?;
        }
        Ok(())
    }
}

impl<T> From<T> for Halt<T> {
    #[inline]
    fn from(inner: T) -> Self {
        Halt {
            inner,
            receiver: Arc::new(Receiver {
                state: Mutex::new(State::Running),
                notify: Condvar::new(),
            }),
        }
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
