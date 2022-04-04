use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Weak, Mutex};
use std::task::{Context, Waker, Poll};

/// The shared contact point between tx and rx.
/// 
/// Both the TX and RX have a reference to an instance of this and "sending" a value is simply
/// putting the value inside the slot.
struct Shared<T> {
    /// Storage for a value that has be sent and not yet received
    pub slot: Option<T>,
    /// The magical waker, I'll explain it just below ;)
    pub waker: Option<Waker>,
}
impl<T> Shared<T> {
    pub fn new() -> Self {
        Self {
            slot: None,
            waker: None,
        }
    }

    /// Extract the internal value, if there is one.
    ///
    /// If there is no value currently, schedule the current task to be woken up when one is sent.
    pub fn poll_get(&mut self, waker: &Waker) -> Option<T> {
        if let Some(ref oldwaker) = self.waker {
            if !waker.will_wake(oldwaker) {
                self.waker.replace(waker.clone());
            }
        } else {
            self.waker = Some(waker.clone());
        }

        self.slot.take()
    }
}

struct TX<T> {
    shared: Weak<Mutex<Shared<T>>>,
}
impl<T> TX<T> {
    pub fn new(shared: Weak<Mutex<Shared<T>>>) -> Self {
        Self { shared }
    }

    /// Set the internal value, in essence "sending" it to the rx.
    pub fn send(&self, value: T) -> Result<(), T> {
        if let Some(shared) = self.shared.upgrade() {
            let mut guard = shared.lock().unwrap();

            if guard.slot.is_some() {
                return Err(value);
            }

            guard.slot = Some(value);
            if let Some(ref waker) = guard.waker {
                waker.wake_by_ref()
            }

            Ok(())
        } else {
            Err(value)
        }
    }
}

struct RX<T> {
    shared: Arc<Mutex<Shared<T>>>,
}
impl<T> RX<T> {
    pub fn new(shared: Arc<Mutex<Shared<T>>>) -> Self {
        Self { shared }
    }
}

fn channel<T>() -> (TX<T>, RX<T>) {
    let shared = Arc::new(Mutex::new(Shared::new()));
    let tx = TX::new(Arc::downgrade(&shared));
    let rx = RX::new(shared);
    (tx, rx)
}

impl<T> Future for RX<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut guard = self.shared.lock().unwrap();
        if let Some(val) = guard.poll_get(cx.waker()) {
            return Poll::Ready(val);
        } else {
            return Poll::Pending;
        }
    }
}

fn main() {
    let (tx, rx) = channel();
    tx.send(42).unwrap();
}
