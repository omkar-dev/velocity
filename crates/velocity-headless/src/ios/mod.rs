//! iOS adapter: Parse XIB/Storyboard XML into RenderTree.

pub mod constraints;
pub mod inflate;
pub mod resources;
pub mod storyboard;
pub mod xib;

pub use inflate::IosInflater;
pub use xib::XibParser;
