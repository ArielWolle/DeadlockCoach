//! Deadlock-specific HUD interpretation built on the reusable perception primitives.
pub mod hud;
pub mod minimap;
pub mod ocr;
pub mod parse;
pub mod scan;
pub mod scoreboard;
pub mod screen;
pub mod vitals;
pub mod team;
pub mod quick;
pub mod localize;
pub mod auto;
pub use coach_perception::matching::{ColorLibrary, Library, Region, Template, ncc};
