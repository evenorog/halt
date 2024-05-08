use crate::{Halt, Remote};
use std::io::{self, Read, Write};

/// A wrapper that makes it possible to pause, stop, and resume iterators, readers, and writers.
#[derive(Debug, Default)]
pub struct Halter<T> {
    inner: T,
    halt: Halt,
}

impl<T> From<T> for Halter<T> {
    fn from(inner: T) -> Self {
        Halter {
            inner,
            halt: Halt::default(),
        }
    }
}

impl<T> Halter<T> {
    /// Returns a remote that allows for pausing, stopping, and resuming.
    pub fn remote(&self) -> Remote {
        self.halt.remote()
    }
}

impl<I: Iterator> Iterator for Halter<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.halt.wait_if_paused();
        if self.halt.is_stopped() {
            None
        } else {
            self.inner.next()
        }
    }
}

impl<I: DoubleEndedIterator> DoubleEndedIterator for Halter<I> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.halt.wait_if_paused();
        if self.halt.is_stopped() {
            None
        } else {
            self.inner.next_back()
        }
    }
}

impl<A, I: Extend<A>> Extend<A> for Halter<I> {
    fn extend<T: IntoIterator<Item = A>>(&mut self, iter: T) {
        self.inner.extend(iter);
    }
}

impl<R: Read> Read for Halter<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.halt.wait_if_paused();
        if self.halt.is_stopped() {
            Ok(0)
        } else {
            self.inner.read(buf)
        }
    }
}

impl<W: Write> Write for Halter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.halt.wait_if_paused();
        if self.halt.is_stopped() {
            Ok(0)
        } else {
            self.inner.write(buf)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
