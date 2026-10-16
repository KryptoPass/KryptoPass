//! Qt application composition. The demo backend is opt-in and contains only invented data.

#[cfg(feature = "demo")]
mod app_info;
#[cfg(feature = "demo")]
pub mod demo;
#[cfg(feature = "demo")]
#[path = "demo/startup.rs"]
mod startup;
