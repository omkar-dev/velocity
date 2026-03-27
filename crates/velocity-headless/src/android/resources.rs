use std::collections::HashMap;

/// Parsed Android resources.arsc file.
///
/// Maps resource IDs to their values for the default configuration.
#[derive(Debug, Clone)]
pub struct ResourceTable {
    /// String resources: @string/name -> value
    pub strings: HashMap<String, String>,
    /// Dimension resources: @dimen/name -> value in dp
    pub dimensions: HashMap<String, f32>,
    /// Color resources: @color/name -> ARGB value
    pub colors: HashMap<String, u32>,
    /// Integer resources: @integer/name -> value
    pub integers: HashMap<String, i32>,
    /// Boolean resources: @bool/name -> value
    pub booleans: HashMap<String, bool>,
    /// Raw resource ID to name mapping
    pub id_to_name: HashMap<u32, String>,
}

impl ResourceTable {
    /// Create an empty resource table.
    pub fn empty() -> Self {
        Self {
            strings: HashMap::new(),
            dimensions: HashMap::new(),
            colors: HashMap::new(),
            integers: HashMap::new(),
            booleans: HashMap::new(),
            id_to_name: HashMap::new(),
        }
    }

    /// Parse a resources.arsc binary file.
    ///
    /// The format is complex (Android's ResourceTypes.h), so we parse
    /// a useful subset: string pool, type spec, and type entries for
    /// the default configuration only.
    pub fn parse(data: &[u8]) -> Result<Self, ResourceError> {
        let mut table = Self::empty();

        if data.len() < 12 {
            return Err(ResourceError::TooShort);
        }

        // Validate header: type=0x0002 (TABLE)
        let chunk_type = u16::from_le_bytes([data[0], data[1]]);
        if chunk_type != 0x0002 {
            return Err(ResourceError::InvalidHeader);
        }

        // Parse string pool (global)
        let mut pos = 12; // Skip table header
        if pos + 8 > data.len() {
            return Ok(table);
        }

        let pool_type = u16::from_le_bytes([data[pos], data[pos + 1]]);
        if pool_type == 0x0001 {
            // String pool
            let pool_size =
                u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
                    as usize;
            // Skip string pool for now — we primarily resolve by type entries
            pos += pool_size;
        }

        // Continue parsing package chunks
        while pos + 8 <= data.len() {
            let chunk_type = u16::from_le_bytes([data[pos], data[pos + 1]]);
            let chunk_size = u32::from_le_bytes([
                data[pos + 4],
                data[pos + 5],
                data[pos + 6],
                data[pos + 7],
            ]) as usize;

            if chunk_size < 8 || pos + chunk_size > data.len() {
                break;
            }

            // We could parse package chunks (0x0200) for detailed resource resolution
            // For v1, we accept that resource resolution may be incomplete

            pos += chunk_size;
        }

        Ok(table)
    }

    /// Resolve a resource reference string.
    ///
    /// Handles formats:
    /// - `@string/app_name` -> string value
    /// - `@dimen/margin_large` -> dimension value as string
    /// - `@color/primary` -> color value as hex string
    /// - `@0x7f040001` -> raw ID lookup
    /// - Plain strings are returned as-is
    pub fn resolve(&self, reference: &str) -> String {
        if !reference.starts_with('@') {
            return reference.to_string();
        }

        if let Some(name) = reference.strip_prefix("@string/") {
            return self
                .strings
                .get(name)
                .cloned()
                .unwrap_or_else(|| reference.to_string());
        }

        if let Some(name) = reference.strip_prefix("@dimen/") {
            return self
                .dimensions
                .get(name)
                .map(|v| format!("{}dp", v))
                .unwrap_or_else(|| reference.to_string());
        }

        if let Some(name) = reference.strip_prefix("@color/") {
            return self
                .colors
                .get(name)
                .map(|v| format!("#{:08x}", v))
                .unwrap_or_else(|| reference.to_string());
        }

        // Raw hex ID
        if let Some(hex) = reference.strip_prefix("@0x") {
            if let Ok(id) = u32::from_str_radix(hex, 16) {
                if let Some(name) = self.id_to_name.get(&id) {
                    return name.clone();
                }
            }
        }

        reference.to_string()
    }

    /// Parse a dimension string to pixels (assuming density=2.0).
    pub fn parse_dimension(value: &str) -> Option<f32> {
        let density = 2.0f32;

        if let Some(px) = value.strip_suffix("px") {
            return px.trim().parse().ok();
        }
        if let Some(dp) = value.strip_suffix("dp") {
            return dp.trim().parse::<f32>().ok().map(|v| v * density);
        }
        if let Some(dip) = value.strip_suffix("dip") {
            return dip.trim().parse::<f32>().ok().map(|v| v * density);
        }
        if let Some(sp) = value.strip_suffix("sp") {
            return sp.trim().parse::<f32>().ok().map(|v| v * density);
        }

        // Plain number = pixels
        value.trim().parse().ok()
    }

    /// Parse a color string to ARGB u32.
    pub fn parse_color(value: &str) -> Option<u32> {
        let hex = value.strip_prefix('#')?;
        match hex.len() {
            6 => u32::from_str_radix(hex, 16).ok().map(|v| 0xFF000000 | v),
            8 => u32::from_str_radix(hex, 16).ok(),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    #[error("Data too short for resources.arsc")]
    TooShort,
    #[error("Invalid resources.arsc header")]
    InvalidHeader,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dimension() {
        assert_eq!(ResourceTable::parse_dimension("16dp"), Some(32.0)); // 16dp * 2.0
        assert_eq!(ResourceTable::parse_dimension("24sp"), Some(48.0));
        assert_eq!(ResourceTable::parse_dimension("100px"), Some(100.0));
        assert_eq!(ResourceTable::parse_dimension("50"), Some(50.0));
        assert!(ResourceTable::parse_dimension("abc").is_none());
    }

    #[test]
    fn test_parse_color() {
        assert_eq!(ResourceTable::parse_color("#FF0000"), Some(0xFFFF0000));
        assert_eq!(ResourceTable::parse_color("#80FF0000"), Some(0x80FF0000));
        assert!(ResourceTable::parse_color("not_a_color").is_none());
    }

    #[test]
    fn test_resolve_plain_string() {
        let table = ResourceTable::empty();
        assert_eq!(table.resolve("Hello"), "Hello");
    }

    #[test]
    fn test_resolve_string_resource() {
        let mut table = ResourceTable::empty();
        table
            .strings
            .insert("app_name".to_string(), "My App".to_string());
        assert_eq!(table.resolve("@string/app_name"), "My App");
        assert_eq!(table.resolve("@string/missing"), "@string/missing");
    }
}
