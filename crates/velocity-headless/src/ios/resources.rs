use std::collections::HashMap;
use std::path::Path;

/// iOS resource loader for asset catalogs and string tables.
pub struct IosResourceLoader {
    /// Localized strings: key -> value.
    pub strings: HashMap<String, String>,
    /// Image names from asset catalogs.
    pub image_names: Vec<String>,
}

impl IosResourceLoader {
    /// Create an empty resource loader.
    pub fn empty() -> Self {
        Self {
            strings: HashMap::new(),
            image_names: Vec::new(),
        }
    }

    /// Load resources from an .app bundle directory.
    pub fn from_app_bundle(app_path: &Path) -> Result<Self, IosResourceError> {
        Self::from_app_bundle_with_locale(app_path, None)
    }

    pub fn from_app_bundle_with_locale(
        app_path: &Path,
        locale: Option<&str>,
    ) -> Result<Self, IosResourceError> {
        let mut loader = Self::empty();

        if let Some(strings_path) = Self::resolve_strings_path(app_path, locale) {
            loader.load_strings_file(&strings_path)?;
        }

        // Scan for .xcassets/Contents.json files
        loader.scan_asset_catalogs(app_path)?;

        Ok(loader)
    }

    fn resolve_strings_path(app_path: &Path, locale: Option<&str>) -> Option<std::path::PathBuf> {
        let mut candidates = Vec::new();
        if let Some(locale) = locale {
            candidates.push(app_path.join(format!("{locale}.lproj/Localizable.strings")));
        }
        candidates.push(app_path.join("Base.lproj/Localizable.strings"));
        candidates.push(app_path.join("en.lproj/Localizable.strings"));

        candidates.into_iter().find(|path| path.exists())
    }

    /// Load a .strings plist file.
    fn load_strings_file(&mut self, path: &Path) -> Result<(), IosResourceError> {
        let data = std::fs::read(path).map_err(|e| IosResourceError::IoError(e.to_string()))?;

        // Try binary plist first, then XML plist
        match plist::from_bytes::<HashMap<String, String>>(&data) {
            Ok(map) => {
                self.strings.extend(map);
                Ok(())
            }
            Err(_) => {
                // Try reading as text key-value format
                let text = String::from_utf8_lossy(&data);
                for line in text.lines() {
                    let line = line.trim();
                    if line.starts_with('"') {
                        if let Some((key, value)) = Self::parse_strings_line(line) {
                            self.strings.insert(key, value);
                        }
                    }
                }
                Ok(())
            }
        }
    }

    /// Parse a single line from .strings format: "key" = "value";
    fn parse_strings_line(line: &str) -> Option<(String, String)> {
        let parts: Vec<&str> = line.splitn(2, '=').collect();
        if parts.len() != 2 {
            return None;
        }

        let key = Self::parse_strings_scalar(parts[0].trim())?;
        let value = Self::parse_strings_scalar(parts[1].trim().trim_end_matches(';').trim())?;

        Some((key, value))
    }

    fn parse_strings_scalar(value: &str) -> Option<String> {
        let trimmed = value.trim();
        if trimmed.len() < 2 || !trimmed.starts_with('"') || !trimmed.ends_with('"') {
            return None;
        }

        let bytes = trimmed.as_bytes();
        if bytes.len() >= 2 {
            let mut backslashes = 0usize;
            for byte in bytes[..bytes.len() - 1].iter().rev() {
                if *byte == b'\\' {
                    backslashes += 1;
                } else {
                    break;
                }
            }
            if backslashes % 2 != 0 {
                return None;
            }
        }

        let inner = &trimmed[1..trimmed.len() - 1];
        let mut result = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars.next() {
                    Some('"') => result.push('"'),
                    Some('\\') => result.push('\\'),
                    Some('n') => result.push('\n'),
                    Some('r') => result.push('\r'),
                    Some('t') => result.push('\t'),
                    Some(other) => {
                        result.push('\\');
                        result.push(other);
                    }
                    None => result.push('\\'),
                }
            } else {
                result.push(ch);
            }
        }

        Some(result)
    }

    /// Scan for asset catalog images.
    fn scan_asset_catalogs(&mut self, app_path: &Path) -> Result<(), IosResourceError> {
        // Look for Assets.car (compiled asset catalog) or .xcassets directories
        let assets_path = app_path.join("Assets.xcassets");
        if assets_path.is_dir() {
            self.scan_xcassets_dir(&assets_path)?;
        }
        Ok(())
    }

    fn scan_xcassets_dir(&mut self, dir: &Path) -> Result<(), IosResourceError> {
        let entries =
            std::fs::read_dir(dir).map_err(|e| IosResourceError::IoError(e.to_string()))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("imageset") {
                if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                    self.image_names.push(name.to_string());
                }
            } else if path.is_dir() {
                self.scan_xcassets_dir(&path)?;
            }
        }

        Ok(())
    }

    /// Resolve a localized string key.
    pub fn localized_string(&self, key: &str) -> Option<&str> {
        self.strings.get(key).map(|s| s.as_str())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IosResourceError {
    #[error("I/O error: {0}")]
    IoError(String),
    #[error("Plist parse error: {0}")]
    PlistError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_strings_line() {
        let result = IosResourceLoader::parse_strings_line(r#""hello_key" = "Hello World";"#);
        assert_eq!(
            result,
            Some(("hello_key".to_string(), "Hello World".to_string()))
        );
    }

    #[test]
    fn test_empty_loader() {
        let loader = IosResourceLoader::empty();
        assert!(loader.strings.is_empty());
        assert!(loader.image_names.is_empty());
        assert!(loader.localized_string("test").is_none());
    }
}
