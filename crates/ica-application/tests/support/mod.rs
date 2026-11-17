//! Minimal executor for use-case tests.
//!
//! ``ica-application`` deliberately has no async runtime in its dependency
//! list (no tokio: the layering rule is checked mechanically), so its tests
//! drive the futures directly. The repository doubles below return
//! immediately-ready futures, so one poll resolves them; the loop exists only
//! to make that assumption explicit rather than silently relying on it.

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

/// Poll a future to completion on the current thread.
///
/// `Waker::noop()` is the standard no-op waker (stable since 1.85); hand-rolling
/// one trips clippy's `manual_noop_waker` lint and is easy to get wrong.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
