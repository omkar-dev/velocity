use std::collections::HashMap;

/// Parsed Android Binary XML document.
///
/// Android compiles XML resources into a binary format (AXML) in the APK.
/// This parser extracts the XML structure from the binary format.
#[derive(Debug, Clone)]
pub struct AxmlDocument {
    pub root: Option<AxmlElement>,
}

/// An element in the parsed AXML tree.
#[derive(Debug, Clone)]
pub struct AxmlElement {
    pub name: String,
    pub namespace: Option<String>,
    pub attributes: HashMap<String, String>,
    pub children: Vec<AxmlElement>,
}

impl AxmlDocument {
    /// Get an attribute value from a named element (searches root).
    pub fn get_attribute(&self, element_name: &str, attr_name: &str) -> Option<String> {
        self.root
            .as_ref()
            .and_then(|root| root.get_attribute(element_name, attr_name))
    }
}

impl AxmlElement {
    /// Get an attribute value from this element or its children.
    pub fn get_attribute(&self, element_name: &str, attr_name: &str) -> Option<String> {
        if self.name == element_name {
            return self.attributes.get(attr_name).cloned();
        }
        for child in &self.children {
            if let Some(val) = child.get_attribute(element_name, attr_name) {
                return Some(val);
            }
        }
        None
    }
}

/// Binary XML chunk types.
const CHUNK_AXML: u16 = 0x0003;
const CHUNK_STRING_POOL: u16 = 0x0001;
const CHUNK_RESOURCE_MAP: u16 = 0x0180;
const CHUNK_START_NAMESPACE: u16 = 0x0100;
const CHUNK_END_NAMESPACE: u16 = 0x0101;
const CHUNK_START_TAG: u16 = 0x0102;
const CHUNK_END_TAG: u16 = 0x0103;

/// Android binary XML parser.
pub struct AxmlParser;

impl AxmlParser {
    /// Parse binary XML data into an AxmlDocument.
    pub fn parse(data: &[u8]) -> Result<AxmlDocument, AxmlError> {
        if data.len() < 8 {
            return Err(AxmlError::TooShort);
        }

        let mut parser = AxmlParserState::new(data);
        parser.parse()?;

        Ok(AxmlDocument {
            root: parser.root,
        })
    }
}

struct AxmlParserState<'a> {
    data: &'a [u8],
    pos: usize,
    strings: Vec<String>,
    resource_ids: Vec<u32>,
    root: Option<AxmlElement>,
    stack: Vec<AxmlElement>,
}

impl<'a> AxmlParserState<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            strings: Vec::new(),
            resource_ids: Vec::new(),
            root: None,
            stack: Vec::new(),
        }
    }

    fn parse(&mut self) -> Result<(), AxmlError> {
        // Read AXML header
        let chunk_type = self.read_u16()?;
        if chunk_type != CHUNK_AXML {
            return Err(AxmlError::InvalidHeader);
        }
        let _header_size = self.read_u16()?;
        let _file_size = self.read_u32()?;

        // Process chunks
        while self.pos < self.data.len() {
            if self.pos + 4 > self.data.len() {
                break;
            }

            let chunk_type = self.read_u16()?;
            let header_size = self.read_u16()?;
            let chunk_size = self.read_u32()? as usize;

            if chunk_size < 8 || self.pos - 8 + chunk_size > self.data.len() {
                break;
            }

            let chunk_data_start = self.pos - 8 + header_size as usize;
            let chunk_end = self.pos - 8 + chunk_size;

            match chunk_type {
                CHUNK_STRING_POOL => {
                    self.parse_string_pool(chunk_data_start, chunk_end)?;
                }
                CHUNK_RESOURCE_MAP => {
                    self.parse_resource_map(chunk_data_start, chunk_end)?;
                }
                CHUNK_START_TAG => {
                    self.parse_start_tag()?;
                    continue; // Don't skip to chunk_end
                }
                CHUNK_END_TAG => {
                    self.parse_end_tag()?;
                    continue;
                }
                CHUNK_START_NAMESPACE | CHUNK_END_NAMESPACE => {
                    // Skip namespace chunks
                }
                _ => {
                    // Unknown chunk, skip
                }
            }

            self.pos = chunk_end;
        }

        Ok(())
    }

    fn parse_string_pool(
        &mut self,
        _data_start: usize,
        chunk_end: usize,
    ) -> Result<(), AxmlError> {
        let string_count = self.read_u32()? as usize;
        let _style_count = self.read_u32()?;
        let flags = self.read_u32()?;
        let strings_start = self.read_u32()? as usize;
        let _styles_start = self.read_u32()?;

        let is_utf8 = (flags & (1 << 8)) != 0;

        // Read string offsets
        let mut offsets = Vec::with_capacity(string_count);
        for _ in 0..string_count {
            offsets.push(self.read_u32()? as usize);
        }

        // The string data starts at strings_start relative to the pool chunk header (pos=8 of chunk)
        // We need to calculate the absolute position
        let pool_data_base = self.pos;

        for &offset in &offsets {
            let abs_pos = pool_data_base + offset;
            if abs_pos >= chunk_end {
                self.strings.push(String::new());
                continue;
            }

            let s = if is_utf8 {
                self.read_utf8_string(abs_pos, chunk_end)
            } else {
                self.read_utf16_string(abs_pos, chunk_end)
            };
            self.strings.push(s);
        }

        self.pos = chunk_end;
        Ok(())
    }

    fn read_utf8_string(&self, pos: usize, limit: usize) -> String {
        if pos + 2 >= limit {
            return String::new();
        }
        // UTF-8 strings have a 2-byte char count, then 2-byte byte count
        let byte_count = self.data.get(pos + 1).copied().unwrap_or(0) as usize;
        let str_start = pos + 2;
        let str_end = (str_start + byte_count).min(limit);
        if str_start >= limit {
            return String::new();
        }
        String::from_utf8_lossy(&self.data[str_start..str_end]).to_string()
    }

    fn read_utf16_string(&self, pos: usize, limit: usize) -> String {
        if pos + 2 >= limit {
            return String::new();
        }
        let char_count = u16::from_le_bytes([
            self.data[pos],
            self.data.get(pos + 1).copied().unwrap_or(0),
        ]) as usize;
        let str_start = pos + 2;

        let mut chars = Vec::with_capacity(char_count);
        let mut p = str_start;
        for _ in 0..char_count {
            if p + 2 > limit {
                break;
            }
            let c = u16::from_le_bytes([self.data[p], self.data[p + 1]]);
            if c == 0 {
                break;
            }
            chars.push(c);
            p += 2;
        }

        String::from_utf16_lossy(&chars)
    }

    fn parse_resource_map(
        &mut self,
        _data_start: usize,
        chunk_end: usize,
    ) -> Result<(), AxmlError> {
        while self.pos + 4 <= chunk_end {
            let val = self.read_u32()?;
            self.resource_ids.push(val);
        }
        Ok(())
    }

    fn parse_start_tag(&mut self) -> Result<(), AxmlError> {
        let _line = self.read_u32()?;
        let _comment = self.read_u32()?;
        let _ns_idx = self.read_i32()?;
        let name_idx = self.read_i32()?;
        let _attr_start = self.read_u16()?;
        let _attr_size = self.read_u16()?;
        let attr_count = self.read_u16()? as usize;
        let _id_index = self.read_u16()?;
        let _class_index = self.read_u16()?;
        let _style_index = self.read_u16()?;

        let name = self.get_string(name_idx as usize);

        let mut attributes = HashMap::new();
        for _ in 0..attr_count {
            let _attr_ns = self.read_i32()?;
            let attr_name_idx = self.read_i32()?;
            let attr_raw_value = self.read_i32()?;
            let _attr_type_size = self.read_u16()?;
            let _attr_type_res = self.read_u8()?;
            let attr_data_type = self.read_u8()?;
            let attr_data = self.read_u32()?;

            let attr_name = self.get_string(attr_name_idx as usize);

            let attr_value = match attr_data_type {
                // String reference
                0x03 => self.get_string(attr_data as usize),
                // Integer decimal
                0x10 => attr_data.to_string(),
                // Integer hex
                0x11 => format!("0x{:08x}", attr_data),
                // Boolean
                0x12 => if attr_data != 0 { "true" } else { "false" }.to_string(),
                // Color
                0x1C | 0x1D => format!("#{:08x}", attr_data),
                // Dimension
                0x05 => {
                    let value = (attr_data >> 8) as f32;
                    let unit = attr_data & 0xFF;
                    match unit {
                        0 => format!("{}px", value),
                        1 => format!("{}dp", value),
                        2 => format!("{}sp", value),
                        _ => format!("{}", value),
                    }
                }
                // Fraction
                0x06 => format!("{}%", (attr_data >> 8) as f32 / 100.0),
                // Resource reference
                0x01 => {
                    if attr_raw_value >= 0 {
                        self.get_string(attr_raw_value as usize)
                    } else {
                        format!("@0x{:08x}", attr_data)
                    }
                }
                // Fallback: try string, then raw value
                _ => {
                    if attr_raw_value >= 0 {
                        self.get_string(attr_raw_value as usize)
                    } else {
                        format!("{}", attr_data)
                    }
                }
            };

            attributes.insert(attr_name, attr_value);
        }

        let element = AxmlElement {
            name,
            namespace: None,
            attributes,
            children: Vec::new(),
        };

        self.stack.push(element);
        Ok(())
    }

    fn parse_end_tag(&mut self) -> Result<(), AxmlError> {
        let _line = self.read_u32()?;
        let _comment = self.read_u32()?;
        let _ns_idx = self.read_i32()?;
        let _name_idx = self.read_i32()?;

        if let Some(element) = self.stack.pop() {
            if let Some(parent) = self.stack.last_mut() {
                parent.children.push(element);
            } else {
                self.root = Some(element);
            }
        }

        Ok(())
    }

    fn get_string(&self, index: usize) -> String {
        self.strings
            .get(index)
            .cloned()
            .unwrap_or_default()
    }

    fn read_u8(&mut self) -> Result<u8, AxmlError> {
        if self.pos >= self.data.len() {
            return Err(AxmlError::UnexpectedEof);
        }
        let val = self.data[self.pos];
        self.pos += 1;
        Ok(val)
    }

    fn read_u16(&mut self) -> Result<u16, AxmlError> {
        if self.pos + 2 > self.data.len() {
            return Err(AxmlError::UnexpectedEof);
        }
        let val = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(val)
    }

    fn read_u32(&mut self) -> Result<u32, AxmlError> {
        if self.pos + 4 > self.data.len() {
            return Err(AxmlError::UnexpectedEof);
        }
        let val = u32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(val)
    }

    fn read_i32(&mut self) -> Result<i32, AxmlError> {
        Ok(self.read_u32()? as i32)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AxmlError {
    #[error("Data too short for AXML")]
    TooShort,
    #[error("Invalid AXML header")]
    InvalidHeader,
    #[error("Unexpected end of data")]
    UnexpectedEof,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_too_short() {
        let result = AxmlParser::parse(&[0, 0, 0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_header() {
        let result = AxmlParser::parse(&[0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert!(result.is_err());
    }
}
