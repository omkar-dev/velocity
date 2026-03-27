use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;

use super::axml::AxmlParser;
use super::resources::ResourceTable;

/// Loads and extracts content from Android APK files.
pub struct ApkLoader {
    /// Parsed resource table from resources.arsc.
    pub resources: ResourceTable,
    /// Raw layout XML files keyed by name (e.g., "activity_main").
    pub layouts: HashMap<String, Vec<u8>>,
    /// The app's package name from AndroidManifest.xml.
    pub package_name: Option<String>,
}

impl ApkLoader {
    /// Load an APK file and extract layouts and resources.
    pub fn from_path(path: &Path) -> Result<Self, ApkError> {
        let file = File::open(path).map_err(|e| ApkError::IoError(e.to_string()))?;
        let mut archive =
            ZipArchive::new(file).map_err(|e| ApkError::ZipError(e.to_string()))?;

        let mut layouts = HashMap::new();
        let mut resources = ResourceTable::empty();
        let mut package_name = None;

        // Collect file names first to avoid borrow issues
        let file_names: Vec<String> = (0..archive.len())
            .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
            .collect();

        for name in &file_names {
            // Extract resources.arsc
            if name == "resources.arsc" {
                let mut entry = archive
                    .by_name(name)
                    .map_err(|e| ApkError::ZipError(e.to_string()))?;
                let mut data = Vec::new();
                entry
                    .read_to_end(&mut data)
                    .map_err(|e| ApkError::IoError(e.to_string()))?;
                resources = ResourceTable::parse(&data).unwrap_or_else(|_| ResourceTable::empty());
            }

            // Extract layout XMLs from res/layout/ directories
            if name.starts_with("res/layout") && name.ends_with(".xml") {
                let mut entry = archive
                    .by_name(name)
                    .map_err(|e| ApkError::ZipError(e.to_string()))?;
                let mut data = Vec::new();
                entry
                    .read_to_end(&mut data)
                    .map_err(|e| ApkError::IoError(e.to_string()))?;

                // Extract layout name from path: res/layout/activity_main.xml -> activity_main
                let layout_name = Path::new(name)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                layouts.insert(layout_name, data);
            }

            // Extract package name from AndroidManifest.xml
            if name == "AndroidManifest.xml" {
                let mut entry = archive
                    .by_name(name)
                    .map_err(|e| ApkError::ZipError(e.to_string()))?;
                let mut data = Vec::new();
                entry
                    .read_to_end(&mut data)
                    .map_err(|e| ApkError::IoError(e.to_string()))?;

                if let Ok(xml) = AxmlParser::parse(&data) {
                    package_name = xml.get_attribute("manifest", "package");
                }
            }
        }

        Ok(Self {
            resources,
            layouts,
            package_name,
        })
    }

    /// Get a parsed layout XML by name.
    pub fn get_layout(&self, name: &str) -> Option<&Vec<u8>> {
        // Strip @layout/ prefix if present
        let clean_name = name
            .strip_prefix("@layout/")
            .unwrap_or(name);
        self.layouts.get(clean_name)
    }

    /// List all available layout names.
    pub fn layout_names(&self) -> Vec<&str> {
        self.layouts.keys().map(|s| s.as_str()).collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ApkError {
    #[error("I/O error: {0}")]
    IoError(String),
    #[error("ZIP error: {0}")]
    ZipError(String),
    #[error("Layout not found: {0}")]
    LayoutNotFound(String),
}
