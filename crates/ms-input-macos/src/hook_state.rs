use ms_input_core::CaptureSink;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

/// Global state reachable from the `CGEventTap` callback. Unlike Windows'
/// hook procedures, `CGEventTap::new`'s callback closure *can* capture
/// state directly (it isn't a bare C function pointer) — but this app only
/// ever runs one capture instance per process, so a global keeps
/// `capture.rs` symmetric with `ms-input-windows` and equally simple to
/// reason about from `ms-core-service`.
pub struct HookState {
    pub sink: RwLock<Option<Arc<dyn CaptureSink>>>,
    pub pass_through: AtomicBool,
}

static STATE: OnceLock<HookState> = OnceLock::new();

pub fn state() -> &'static HookState {
    STATE.get_or_init(|| HookState { sink: RwLock::new(None), pass_through: AtomicBool::new(true) })
}

pub fn set_sink(sink: Option<Arc<dyn CaptureSink>>) {
    *state().sink.write().expect("hook state poisoned") = sink;
}

pub fn with_sink(f: impl FnOnce(&Arc<dyn CaptureSink>)) {
    if let Some(sink) = state().sink.read().expect("hook state poisoned").as_ref() {
        f(sink);
    }
}

pub fn pass_through() -> bool {
    state().pass_through.load(Ordering::Acquire)
}

pub fn set_pass_through(value: bool) {
    state().pass_through.store(value, Ordering::Release);
}
