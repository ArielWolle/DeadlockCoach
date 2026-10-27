//! Game-independent, profile-driven perception. Images enter; inspectable observations leave.
//! This crate has no dependency on game snapshots, coaching policies, capture or audio.
pub mod matching;
pub mod localization;
#[cfg(feature = "ocrs-backend")]
pub mod ocr;
pub mod pipeline;
pub mod track;
pub use pipeline::*;
