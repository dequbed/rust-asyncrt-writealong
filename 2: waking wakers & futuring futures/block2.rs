// This code too can be compiled with normal rustc (if you pass --edition=2018
// or 2021 at least).
// 
// NOTE: This code never stops, you need to send it a signal like SIGQUIT or
// SIGINT (Ctrl-C).

use std::sync::{Arc, Mutex};
use std::pin::Pin;
use std::future::Future;
use std::task::{Context, Waker, Poll, RawWaker, RawWakerVTable};
use std::sync::mpsc::{Receiver, Sender, channel};

mod block1;

/// A struct representing an instance of our runtime doing things
struct OurRuntime {
    /// There's lots of smart fancy queing systems aroung. But mpsc::Receiver is
    /// simple and it allows us to block.
    queue: Receiver<Pin<Box<dyn Future<Output=()>>>>,
    spawn: Sender<Pin<Box<dyn Future<Output=()>>>>,
}
impl OurRuntime {
    pub fn new() -> Self {
        let (spawn, queue) = channel();
        Self {
            queue, spawn
        }
    }

    /// So, to spawn a future, just send it on the sending end.
    pub fn spawn(&self, f: Pin<Box<dyn Future<Output=()>>>) {
        self.spawn.send(f).unwrap()
    }

    /// Execute a schduled future until it yields once. Blocks until there is a
    /// future to be ran.
    pub fn tick(&mut self) {
        // We construct our fancy Waker here. 
        let waker: Arc<OurWaker> = OurWaker::new(self.spawn.clone());

        // This is where using mpsc channels is nice. recv() will block until
        // there is more work available, stopping us from having to worry about
        // that (at least until the next post! ;p )
        let mut f = self.queue.recv().unwrap();

        if {
            // Prepare all the things required to poll a future:
            // Make our custom Waker into a `std::task::Waker`.
            let waker = unsafe { 
                Waker::from_raw(OurWaker::into_raw_waker(waker.clone())) 
            };
            // Build a Context from said waker
            let mut cx = Context::from_waker(&waker);

            // poll() and return true if we need to run this fut again
            f.as_mut().poll(&mut cx).is_pending()
        } {
            // If we do need to run the future again, we store it in our Waker
            // so it can send it to the runtime when `wake()` is called.
            waker.replace(f);
        }
    }

    pub fn run_forever(&mut self) {
        loop {
            self.tick()
        }
    }
}

/// Custom waker that takes (temporary) ownership of the future when it's
/// blocked on something and spawns it again when `wake()` is called.
///
/// This is not efficient, but I think it's one of the easiest approaches to
/// understand.
struct OurWaker {
    spawn: Sender<Pin<Box<dyn Future<Output=()>>>>,
    slot: Mutex<Option<Pin<Box<dyn Future<Output=()>>>>>,
}

impl OurWaker {
    /// When we are created initially we don't have a future yet since that
    /// needs to be polled first.
    pub fn new(spawn: Sender<Pin<Box<dyn Future<Output=()>>>>) -> Arc<Self> {
        Arc::new(Self {
            spawn,
            slot: Mutex::new(None),
        })
    }

    /// Actually set the future to be maybe rescheduled
    pub fn replace(&self, f: Pin<Box<dyn Future<Output=()>>>) {
        let mut guard = self.slot.lock().unwrap();
        guard.replace(f);
    }

    /// Scheduling the future by just spawning it like a new one. it works.
    pub fn reshedule(&self) {
        let mut guard = self.slot.lock().unwrap();
        if let Some(fut) = guard.take() {
            self.spawn.send(fut);
        }
    }

    //
    // RawWaker implementation code
    //
    // We create the RawWaker from an **Arc<Self>**, not a Self directly. So the
    // methods below make heavy use of `Arc::from_raw` and `Arc::into_raw`
    //

    /// Convert an **Arc<Self>** into a RawWaker
    pub fn into_raw_waker(this: Arc<Self>) -> RawWaker {
        let ptr = Arc::into_raw(this);
        RawWaker::new(ptr.cast(), &OUR_VTABLE)
    }

    /// Convert an RawWaker into an **Arc<Self>**
    unsafe fn from_raw_waker(ptr: *const()) -> Arc<Self> {
        Arc::from_raw(ptr.cast())
    }

    pub unsafe fn clone(ptr: *const ()) -> RawWaker {
        let ptr: *const Self = ptr.cast();
        Arc::increment_strong_count(ptr);
        RawWaker::new(ptr.cast(), &OUR_VTABLE)
    }

    pub unsafe fn wake(ptr: *const ()) {
        let this = Self::from_raw_waker(ptr);
        this.reshedule()
    }

    pub unsafe fn wake_by_ref(ptr: *const ()) {
        Self::clone(ptr);
        Self::wake(ptr);
    }

    pub unsafe fn drop(ptr: *const ()) {
        let ptr: *const Self = ptr.cast();
        Arc::decrement_strong_count(ptr)
    }
}

static OUR_VTABLE: RawWakerVTable = RawWakerVTable::new(
    OurWaker::clone,
    OurWaker::wake,
    OurWaker::wake_by_ref,
    OurWaker::drop,
);


pub fn main() {
    let mut rt = OurRuntime::new();

    let (tx, rx) = block1::channel();
    rt.spawn(Box::pin(async {
        println!("RX1: I'm being polled for the first time!");
        let v = rx.await;
        println!("RX1: I received a {}", v);
    }));
    let (tx2, rx2) = block1::channel();
    rt.spawn(Box::pin(async {
        println!("RX2: I'm being polled for the first time!");
        let v = rx2.await;
        println!("RX2: I received a {}", v);
    }));

    std::thread::spawn(move || {
        println!("Spawning a new thread, sleeping for 2 seconds first");
        std::thread::sleep(std::time::Duration::from_secs(2));

        println!("let's send to the second future *first*! (And then sleep again for good measure)");
        tx2.try_send(36).unwrap();

        std::thread::sleep(std::time::Duration::from_secs(2));

        println!("Yaaaawn, now sending an 42 to the first future");
        tx.try_send(42).unwrap();
    });
    rt.run_forever();
}
