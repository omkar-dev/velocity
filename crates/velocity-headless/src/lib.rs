//! Velocity Headless Renderer
//!
//! CPU-only rendering engine for Android XML layouts and iOS XIB/Storyboard files.
//! Enables layout assertions in ~50ms without emulators or simulators.
//!
//! # Architecture
//!
//! Two-layer design:
//! 1. **Core Layer** (platform-agnostic): Taffy flexbox layout + tiny-skia pixel rendering
//! 2. **Platform Adapters**: Android AXML parser + iOS XIB parser → RenderTree
//!
//! # Usage
//!
//! ```rust,ignore
//! use velocity_headless::HeadlessDriver;
//! use velocity_common::Platform;
//!
//! let driver = HeadlessDriver::new(Platform::Android, HeadlessConfig::default());
//! ```

// Core rendering layer
pub mod element_map;
pub mod layout;
pub mod render_tree;
pub mod surface;
pub mod text;

// Platform adapters
pub mod android;
pub mod ios;

// Driver integration
pub mod config;
pub mod driver;
pub mod session;
pub mod snapshot;

// Public re-exports
pub use config::HeadlessConfig;
pub use driver::HeadlessDriver;
pub use render_tree::{Color, ComputedLayout, EdgeSizes, NodeStyle, RenderNode};
pub use surface::SoftwareSurface;
