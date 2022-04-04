// Our only dependencies are the Rust stdlib; you can compile and run this code just by calling
// `$ rust block1.rs` and then run the resulting executable named `block1`. The Makefile in this
// directory does just that for you.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Weak, Mutex};
use std::task::{Context, Waker, Poll};

#[derive(Debug)]
/// The shared contact point between tx and rx.
/// 
/// Both the TX and RX have a reference to an instance of this and "sending" a value is simply
/// putting the value inside the slot.
struct Shared<T> {
    /// Storage for a value that has be sent and not yet received
    pub slot: Option<T>,
    /// The mystical Waker, the part making everything smartly async (◕▿◕✿)
    pub waker: Option<Waker>,
}
impl<T> Shared<T> {
    pub fn new() -> Self {
        Self {
            slot: None,
            waker: None,
        }
    }
}

#[derive(Debug)]
/// Transmitting end
///
/// For simplicity this is a very simple and entirely syncronous implementation.
/// A Weak pointer is used so that shared is deallocated as soon as the RX is dropped.
struct TX<T> {
    shared: Weak<Mutex<Shared<T>>>,
}
impl<T> TX<T> {
    pub fn new(shared: Weak<Mutex<Shared<T>>>) -> Self {
        Self { shared }
    }

    /// Filling the shared slot, in essence "sending" it to the rx.
    pub fn try_send(&self, value: T) -> Result<(), T> {
        if let Some(shared) = self.shared.upgrade() {
            let mut guard = shared.lock().unwrap();

            if guard.slot.is_some() {
                return Err(value);
            }

            guard.slot = Some(value);
            if let Some(waker) = guard.waker.take() {
                waker.wake()
            }

            Ok(())
        } else {
            // This case is hit when the corresponding receiving end has been dropped. To not make
            // things complicated we don't return a specific type for this kind of error.
            Err(value)
        }
    }

    /// Returns true if the RX end has not yet been dropped.
    pub fn is_rx_alive(&self) -> bool {
        self.shared.strong_count() > 0
    }
}

#[derive(Debug)]
struct RX<T> {
    shared: Arc<Mutex<Shared<T>>>,
}
impl<T> RX<T> {
    pub fn new(shared: Arc<Mutex<Shared<T>>>) -> Self {
        Self { shared }
    }

    /// Extract the internal value, if there is one.
    ///
    /// If there is no value currently, **schedule the current task to be woken up** when one is sent.
    pub fn poll_get(&self, waker: &Waker) -> Option<T> {
        let mut guard = self.shared.lock().unwrap();

        if let Some(ref oldwaker) = guard.waker {
            if !waker.will_wake(oldwaker) {
                guard.waker.replace(waker.clone());
            }
        } else {
            guard.waker = Some(waker.clone());
        }

        guard.slot.take()
    }
}

impl<T> Future for RX<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(val) = self.poll_get(cx.waker()) {
            return Poll::Ready(val);
        } else {
            return Poll::Pending;
        }
    }
}

fn channel<T>() -> (TX<T>, RX<T>) {
    let shared = Arc::new(Mutex::new(Shared::new()));
    let tx = TX::new(Arc::downgrade(&shared));
    let rx = RX::new(shared);
    (tx, rx)
}

fn main() {
    let (tx, rx) = channel();
    tx.try_send(42).unwrap();
    println!("I sent a value!");
    println!("Now the endpoints look like this: {:?} and {:?}", tx, rx);
}
