//! Capture layer - continuous, offline, cheap
//!
//! This layer runs constantly and never touches the network.
//! It grabs raw screenshots and basic window metadata.

pub mod events;
pub mod screenshot;
pub mod window;

// Re-export types
pub use events::EventListener;
pub use screenshot::ScreenshotCapture;
pub use window::WindowMetadata;
