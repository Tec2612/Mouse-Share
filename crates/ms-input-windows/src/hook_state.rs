use ms_input_core::CaptureSink;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

/// `WH_MOUSE_LL`/`WH_KEYBOARD_LL` hook procedures are plain
/// `extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT` — the Win32 API
/// gives them no user-data slot to carry a `self` pointer through, unlike
/// window procedures (which at least have an associated `HWND` you can
/// stash data on via `SetWindowLongPtr`). Global process state is
/// therefore the documented pattern for low-level hooks, not a shortcut
/// taken here; there is at most one active capture instance per process
/// in this app regardless.
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
