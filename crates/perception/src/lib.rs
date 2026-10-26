//! Game-independent, profile-driven perception. Images enter; inspectable observations leave.
//! This crate has no dependency on game snapshots, coaching policies, capture or audio.
pub mod matching;
#[cfg(feature = "ocrs-backend")]
pub mod ocr;
pub mod pipeline;
pub use pipeline::*;
