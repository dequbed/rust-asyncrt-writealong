use std::sync::{Arc, Mutex};
use std::pin::Pin;
use std::future::Future;
use std::task::{Context, Waker, Poll, RawWaker, RawWakerVTable};
use std::sync::mpsc::{Receiver, Sender, channel};

mod block1;

struct OurRuntime {
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

    pub fn spawn(&self, f: Pin<Box<dyn Future<Output=()>>>) {
        self.spawn.send(f).unwrap()
    }

    /// Execute a schduled future until it yields once. Blocks until there is a future to be ran.
    pub fn tick(&mut self) {
        let waker: Arc<OurWaker> = OurWaker::new(self.spawn.clone());
        let mut f = self.queue.recv().unwrap();
        if {
            let waker = unsafe { Waker::from_raw(OurWaker::into_raw_waker(waker.clone())) };
            let mut cx = Context::from_waker(&waker);
            // return true if we need to run this fut again
            f.as_mut().poll(&mut cx).is_pending()
        } {
            waker.replace(f);
        }
    }

    pub fn run_forever(&mut self) {
        loop {
            self.tick()
        }
    }
}

struct OurWaker {
    spawn: Sender<Pin<Box<dyn Future<Output=()>>>>,
    slot: Mutex<Option<Pin<Box<dyn Future<Output=()>>>>>,
}

impl OurWaker {
    pub fn new(spawn: Sender<Pin<Box<dyn Future<Output=()>>>>) -> Arc<Self> {
        Arc::new(Self {
            spawn,
            slot: Mutex::new(None),
        })
    }

    pub fn replace(&self, f: Pin<Box<dyn Future<Output=()>>>) {
        let mut guard = self.slot.lock().unwrap();
        guard.replace(f);
    }

    pub fn reshedule(&self) {
        let mut guard = self.slot.lock().unwrap();
        if let Some(fut) = guard.take() {
            self.spawn.send(fut);
        }
    }

    pub fn into_raw_waker(this: Arc<Self>) -> RawWaker {
        let ptr = Arc::into_raw(this);
        RawWaker::new(ptr.cast(), &OUR_VTABLE)
    }

    unsafe fn from_raw_waker(ptr: *const()) -> Arc<Self> {
        Arc::from_raw(ptr.cast())
    }

    // We create the RawWaker from an **Arc<Self>**, not a Self directly.

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
        println!("I'm being polled for the first time!");
        let v = rx.await;
        println!("I received a {}", v);
    }));

    std::thread::spawn(move || {
        println!("Spawning a new thread");
        std::thread::sleep(std::time::Duration::from_secs(5));
        println!("Yaaaawn, sending an 42");
        tx.try_send(42).unwrap();
        println!("Sent a 42");
    });
    rt.run_forever();
}
