//! Android adapter: Parse APK binary XML layouts into RenderTree.

pub mod apk;
pub mod axml;
pub mod inflate;
pub mod resources;
pub mod styles;

pub use apk::ApkLoader;
pub use inflate::AndroidInflater;
