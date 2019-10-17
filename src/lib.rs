use std::sync::{Condvar, Mutex, Weak};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum State {
    Running,
    Paused,
}

#[derive(Debug)]
pub struct Controller {
    halt: Weak<Receiver>,
}

impl Controller {
    #[inline]
    pub fn pause(&self) {
        if let Some(halt) = self.halt.upgrade() {
            let mut guard = halt.state.lock().unwrap();
            *guard = State::Paused;
        }
    }

    #[inline]
    pub fn resume(&self) {
        if let Some(halt) = self.halt.upgrade() {
            let mut guard = halt.state.lock().unwrap();
            *guard = State::Running;
            halt.notify.notify_all();
        }
    }
}

#[derive(Debug)]
pub struct Receiver {
    state: Mutex<State>,
    notify: Condvar,
}

impl Receiver {
    #[inline]
    pub fn wait_if_paused(&self) {
        let mut guard = self.state.lock().unwrap();
        while *guard == State::Paused {
            guard = self.notify.wait(guard).unwrap();
        }
    }
}
