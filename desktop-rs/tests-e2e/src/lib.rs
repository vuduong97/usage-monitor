//! Headless end-to-end harness (phase 14). Library level only: the engine's pieces over a temp
//! home, loopback fakes, never a window, tray, WebView, the real keychain/registry or port 47913.
//! The scenarios are in `tests/`.

pub mod engine_rig;
pub mod http;
pub mod rig;
