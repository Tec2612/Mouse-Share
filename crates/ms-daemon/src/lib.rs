//! Shared daemon logic reused by both the headless binary (`main.rs`) and
//! the Tauri UI backend (`app/src-tauri`), which embeds this crate as a
//! library rather than talking to the headless binary over IPC — see
//! `docs/architecture.md`. Platform input wiring (`input.rs`) stays
//! binary-only: it's specific to how the headless daemon starts capture
//! at process launch, whereas the UI process manages that lifecycle
//! itself (e.g. to react to onboarding permission grants live).

pub mod identity;
pub mod network;
