//! # xml2abx
//!
//! A library for converting XML to Android Binary XML format.
//!
//! ## Example
//!
//! ```rust
//! use xml2abx::XmlToAbxConverter;
//! use std::io::Cursor;
//!
//! let xml = r#"<?xml version="1.0"?>
//! <root>
//!     <element attr="value">text content</element>
//! </root>"#;
//!
//! let mut output = Vec::new();
//! XmlToAbxConverter::convert_from_string(xml, &mut output).unwrap();
//! ```

use byteorder::{BigEndian, WriteBytesExt};
use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConversionError {
    #[error("XML parsing failed: {0}")]
    XmlParsing(#[from] quick_xml::Error),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("String too long: {0} bytes (max: {1})")]
    StringTooLong(usize, usize),
    #[error("Binary data too long: {0} bytes (max: {1})")]
    BinaryDataTooLong(usize, usize),
    #[error("Invalid hex string")]
    InvalidHex,
    #[error("Invalid base64 string")]
    InvalidBase64,
    #[error("UTF-8 conversion error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),
    #[error("Attribute error: {0}")]
    AttrError(#[from] quick_xml::events::attributes::AttrError),
}

/// show a warning about unsupported features
pub fn show_warning(feature: &str, details: Option<&str>) {
    eprintln!("WARNING: {} is not supported and might be lost.", feature);
    if let Some(details) = details {
        eprintln!("  {}", details);
    }
}

pub struct FastDataOutput<W: Write> {
    writer: W,
    string_pool: HashMap<String, u16>,
    interned_strings: Vec<String>,
}

impl<W: Write> FastDataOutput<W> {
    pub const MAX_UNSIGNED_SHORT: u16 = 65535;

    pub fn new(writer: W) -> Self {
        Self {
            writer,
            string_pool: HashMap::new(),
            interned_strings: Vec::new(),
        }
    }

    pub fn write_byte(&mut self, value: u8) -> Result<(), ConversionError> {
        self.writer.write_u8(value)?;
        Ok(())
    }

    pub fn write_short(&mut self, value: u16) -> Result<(), ConversionError> {
        self.writer.write_u16::<BigEndian>(value)?;
        Ok(())
    }

    pub fn write_int(&mut self, value: i32) -> Result<(), ConversionError> {
        self.writer.write_i32::<BigEndian>(value)?;
        Ok(())
    }

    pub fn write_long(&mut self, value: i64) -> Result<(), ConversionError> {
        self.writer.write_i64::<BigEndian>(value)?;
        Ok(())
    }

    pub fn write_float(&mut self, value: f32) -> Result<(), ConversionError> {
        self.writer.write_f32::<BigEndian>(value)?;
        Ok(())
    }

    pub fn write_double(&mut self, value: f64) -> Result<(), ConversionError> {
        self.writer.write_f64::<BigEndian>(value)?;
        Ok(())
    }

    pub fn write_utf(&mut self, s: &str) -> Result<(), ConversionError> {
        let bytes = s.as_bytes();
        if bytes.len() > Self::MAX_UNSIGNED_SHORT as usize {
            return Err(ConversionError::StringTooLong(
                bytes.len(),
                Self::MAX_UNSIGNED_SHORT as usize,
            ));
        }
        self.write_short(bytes.len() as u16)?;
        self.writer.write_all(bytes)?;
        Ok(())
    }

    pub fn write_interned_utf(&mut self, s: &str) -> Result<(), ConversionError> {
        if let Some(&index) = self.string_pool.get(s) {
            self.write_short(index)?;
        } else {
            self.write_short(0xFFFF)?;
            self.write_utf(s)?;
            let index = self.interned_strings.len() as u16;
            self.string_pool.insert(s.to_string(), index);
            self.interned_strings.push(s.to_string());
        }
        Ok(())
    }

    pub fn write_bytes(&mut self, data: &[u8]) -> Result<(), ConversionError> {
        self.writer.write_all(data)?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), ConversionError> {
        self.writer.flush()?;
        Ok(())
    }
}

pub struct BinaryXmlSerializer<W: Write> {
    output: FastDataOutput<W>,
    tag_count: usize,
    tag_names: Vec<String>,
    preserve_whitespace: bool,
}

// Constants
impl<W: Write> BinaryXmlSerializer<W> {
    pub const PROTOCOL_MAGIC_VERSION_0: [u8; 4] = [0x41, 0x42, 0x58, 0x00];
    pub const START_DOCUMENT: u8 = 0;
    pub const END_DOCUMENT: u8 = 1;
    pub const START_TAG: u8 = 2;
    pub const END_TAG: u8 = 3;
    pub const TEXT: u8 = 4;
    pub const CDSECT: u8 = 5;
    pub const ENTITY_REF: u8 = 6;
    pub const IGNORABLE_WHITESPACE: u8 = 7;
    pub const PROCESSING_INSTRUCTION: u8 = 8;
    pub const COMMENT: u8 = 9;
    pub const DOCDECL: u8 = 10;
    pub const ATTRIBUTE: u8 = 15;

    pub const TYPE_NULL: u8 = 1 << 4;
    pub const TYPE_STRING: u8 = 2 << 4;
    pub const TYPE_STRING_INTERNED: u8 = 3 << 4;
    pub const TYPE_BYTES_HEX: u8 = 4 << 4;
    pub const TYPE_BYTES_BASE64: u8 = 5 << 4;
    pub const TYPE_INT: u8 = 6 << 4;
    pub const TYPE_INT_HEX: u8 = 7 << 4;
    pub const TYPE_LONG: u8 = 8 << 4;
    pub const TYPE_LONG_HEX: u8 = 9 << 4;
    pub const TYPE_FLOAT: u8 = 10 << 4;
    pub const TYPE_DOUBLE: u8 = 11 << 4;
    pub const TYPE_BOOLEAN_TRUE: u8 = 12 << 4;
    pub const TYPE_BOOLEAN_FALSE: u8 = 13 << 4;

    pub fn new(writer: W) -> Result<Self, ConversionError> {
        Self::with_options(writer, true)
    }

    pub fn with_options(writer: W, preserve_whitespace: bool) -> Result<Self, ConversionError> {
        let mut output = FastDataOutput::new(writer);
        output.write_bytes(&Self::PROTOCOL_MAGIC_VERSION_0)?;

        Ok(Self {
            output,
            tag_count: 0,
            tag_names: Vec::with_capacity(8),
            preserve_whitespace,
        })
    }

    fn write_token(&mut self, token: u8, text: Option<&str>) -> Result<(), ConversionError> {
        if let Some(text) = text {
            self.output.write_byte(token | Self::TYPE_STRING)?;
            self.output.write_utf(text)?;
        } else {
            self.output.write_byte(token | Self::TYPE_NULL)?;
        }
        Ok(())
    }

    pub fn start_document(&mut self) -> Result<(), ConversionError> {
        self.output
            .write_byte(Self::START_DOCUMENT | Self::TYPE_NULL)
    }

    pub fn end_document(&mut self) -> Result<(), ConversionError> {
        self.output
            .write_byte(Self::END_DOCUMENT | Self::TYPE_NULL)?;
        self.output.flush()
    }

    pub fn start_tag(&mut self, name: &str) -> Result<(), ConversionError> {
        if self.tag_count == self.tag_names.len() {
            let new_size = self.tag_count + std::cmp::max(1, self.tag_count / 2);
            self.tag_names.resize(new_size, String::new());
        }
        self.tag_names[self.tag_count] = name.to_string();
        self.tag_count += 1;

        self.output
            .write_byte(Self::START_TAG | Self::TYPE_STRING_INTERNED)?;
        self.output.write_interned_utf(name)
    }

    pub fn end_tag(&mut self, name: &str) -> Result<(), ConversionError> {
        self.tag_count -= 1;
        self.output
            .write_byte(Self::END_TAG | Self::TYPE_STRING_INTERNED)?;
        self.output.write_interned_utf(name)
    }

    pub fn attribute(&mut self, name: &str, value: &str) -> Result<(), ConversionError> {
        self.output
            .write_byte(Self::ATTRIBUTE | Self::TYPE_STRING)?;
        self.output.write_interned_utf(name)?;
        self.output.write_utf(value)
    }

    pub fn attribute_interned(&mut self, name: &str, value: &str) -> Result<(), ConversionError> {
        self.output
            .write_byte(Self::ATTRIBUTE | Self::TYPE_STRING_INTERNED)?;
        self.output.write_interned_utf(name)?;
        self.output.write_interned_utf(value)
    }

    pub fn attribute_bytes_hex(&mut self, name: &str, value: &[u8]) -> Result<(), ConversionError> {
        if value.len() > FastDataOutput::<W>::MAX_UNSIGNED_SHORT as usize {
            return Err(ConversionError::BinaryDataTooLong(
                value.len(),
                FastDataOutput::<W>::MAX_UNSIGNED_SHORT as usize,
            ));
        }
        self.output
            .write_byte(Self::ATTRIBUTE | Self::TYPE_BYTES_HEX)?;
        self.output.write_interned_utf(name)?;
        self.output.write_short(value.len() as u16)?;
        self.output.write_bytes(value)
    }

    pub fn attribute_bytes_base64(
        &mut self,
        name: &str,
        value: &[u8],
    ) -> Result<(), ConversionError> {
        if value.len() > FastDataOutput::<W>::MAX_UNSIGNED_SHORT as usize {
            return Err(ConversionError::BinaryDataTooLong(
                value.len(),
                FastDataOutput::<W>::MAX_UNSIGNED_SHORT as usize,
            ));
        }
        self.output
            .write_byte(Self::ATTRIBUTE | Self::TYPE_BYTES_BASE64)?;
        self.output.write_interned_utf(name)?;
        self.output.write_short(value.len() as u16)?;
        self.output.write_bytes(value)
    }

    pub fn attribute_int(&mut self, name: &str, value: i32) -> Result<(), ConversionError> {
        self.output.write_byte(Self::ATTRIBUTE | Self::TYPE_INT)?;
        self.output.write_interned_utf(name)?;
        self.output.write_int(value)
    }

    pub fn attribute_int_hex(&mut self, name: &str, value: i32) -> Result<(), ConversionError> {
        self.output
            .write_byte(Self::ATTRIBUTE | Self::TYPE_INT_HEX)?;
        self.output.write_interned_utf(name)?;
        self.output.write_int(value)
    }

    pub fn attribute_long(&mut self, name: &str, value: i64) -> Result<(), ConversionError> {
        self.output.write_byte(Self::ATTRIBUTE | Self::TYPE_LONG)?;
        self.output.write_interned_utf(name)?;
        self.output.write_long(value)
    }

    pub fn attribute_long_hex(&mut self, name: &str, value: i64) -> Result<(), ConversionError> {
        self.output
            .write_byte(Self::ATTRIBUTE | Self::TYPE_LONG_HEX)?;
        self.output.write_interned_utf(name)?;
        self.output.write_long(value)
    }

    pub fn attribute_float(&mut self, name: &str, value: f32) -> Result<(), ConversionError> {
        self.output.write_byte(Self::ATTRIBUTE | Self::TYPE_FLOAT)?;
        self.output.write_interned_utf(name)?;
        self.output.write_float(value)
    }

    pub fn attribute_double(&mut self, name: &str, value: f64) -> Result<(), ConversionError> {
        self.output
            .write_byte(Self::ATTRIBUTE | Self::TYPE_DOUBLE)?;
        self.output.write_interned_utf(name)?;
        self.output.write_double(value)
    }

    pub fn attribute_boolean(&mut self, name: &str, value: bool) -> Result<(), ConversionError> {
        let token = if value {
            Self::ATTRIBUTE | Self::TYPE_BOOLEAN_TRUE
        } else {
            Self::ATTRIBUTE | Self::TYPE_BOOLEAN_FALSE
        };
        self.output.write_byte(token)?;
        self.output.write_interned_utf(name)
    }

    pub fn text(&mut self, text: &str) -> Result<(), ConversionError> {
        self.write_token(Self::TEXT, Some(text))
    }

    pub fn cdsect(&mut self, text: &str) -> Result<(), ConversionError> {
        self.write_token(Self::CDSECT, Some(text))
    }

    pub fn comment(&mut self, text: &str) -> Result<(), ConversionError> {
        self.write_token(Self::COMMENT, Some(text))
    }

    pub fn processing_instruction(
        &mut self,
        target: &str,
        data: Option<&str>,
    ) -> Result<(), ConversionError> {
        let full_pi = if let Some(data) = data {
            if data.is_empty() {
                target.to_string()
            } else {
                format!("{} {}", target, data)
            }
        } else {
            target.to_string()
        };
        self.write_token(Self::PROCESSING_INSTRUCTION, Some(&full_pi))
    }

    pub fn docdecl(&mut self, text: &str) -> Result<(), ConversionError> {
        self.write_token(Self::DOCDECL, Some(text))
    }

    pub fn ignorable_whitespace(&mut self, text: &str) -> Result<(), ConversionError> {
        self.write_token(Self::IGNORABLE_WHITESPACE, Some(text))
    }

    pub fn entity_ref(&mut self, text: &str) -> Result<(), ConversionError> {
        self.write_token(Self::ENTITY_REF, Some(text))
    }
}

mod type_detection {
    /// only detects truly unambiguous cases ->> scientific notation doubles
    pub fn is_scientific_notation(s: &str) -> bool {
        s.contains('e') || s.contains('E')
    }

    /// 0nly "true" or "false" ->> unambiguous boolean
    pub fn is_boolean(s: &str) -> bool {
        s == "true" || s == "false"
    }

    pub fn is_whitespace_only(s: &str) -> bool {
        s.chars().all(|c| c.is_whitespace())
    }
}

pub struct XmlToAbxConverter;

impl XmlToAbxConverter {
    pub fn convert_from_string<W: Write>(xml: &str, writer: W) -> Result<(), ConversionError> {
        Self::convert_from_string_with_options(xml, writer, true)
    }

    pub fn convert_from_string_with_options<W: Write>(
        xml: &str,
        writer: W,
        preserve_whitespace: bool,
    ) -> Result<(), ConversionError> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(!preserve_whitespace);
        Self::convert_reader_with_options(reader, writer, preserve_whitespace)
    }

    pub fn convert_from_file<W: Write>(input_path: &str, writer: W) -> Result<(), ConversionError> {
        Self::convert_from_file_with_options(input_path, writer, true)
    }

    pub fn convert_from_file_with_options<W: Write>(
        input_path: &str,
        writer: W,
        preserve_whitespace: bool,
    ) -> Result<(), ConversionError> {
        let mut reader = Reader::from_file(input_path)?;
        reader.config_mut().trim_text(!preserve_whitespace);
        Self::convert_reader_with_options(reader, writer, preserve_whitespace)
    }

    pub fn convert_from_reader<R: BufRead, W: Write>(
        input: R,
        writer: W,
    ) -> Result<(), ConversionError> {
        Self::convert_from_reader_with_options(input, writer, true)
    }

    pub fn convert_from_reader_with_options<R: BufRead, W: Write>(
        input: R,
        writer: W,
        preserve_whitespace: bool,
    ) -> Result<(), ConversionError> {
        let mut reader = Reader::from_reader(input);
        reader.config_mut().trim_text(!preserve_whitespace);
        Self::convert_reader_with_options(reader, writer, preserve_whitespace)
    }

    fn convert_reader_with_options<R: BufRead, W: Write>(
        mut reader: Reader<R>,
        writer: W,
        preserve_whitespace: bool,
    ) -> Result<(), ConversionError> {
        let mut serializer = BinaryXmlSerializer::with_options(writer, preserve_whitespace)?;
        let mut buf = Vec::new();
        let mut tag_stack = Vec::new();

        serializer.start_document()?;

        loop {
            match reader.read_event_into(&mut buf)? {
                Event::Start(e) => {
                    let name_bytes = e.name();
                    let name = std::str::from_utf8(name_bytes.as_ref())?;
                    if name.contains(':') {
                        show_warning(
                            "Namespaces and prefixes",
                            Some(&format!("Found prefixed element: {}", name)),
                        );
                    }

                    serializer.start_tag(name)?;
                    tag_stack.push(name.to_string());
                    for attr in e.attributes() {
                        let attr = attr?;
                        let attr_name = std::str::from_utf8(attr.key.as_ref())?;
                        let attr_value = std::str::from_utf8(&attr.value)?;
                        if attr_name.starts_with("xmlns") || attr_name.contains(':') {
                            show_warning(
                                "Namespaces and prefixes",
                                Some(&format!(
                                    "Found namespace declaration or prefixed attribute: {}",
                                    attr_name
                                )),
                            );
                        }

                        Self::write_attribute(&mut serializer, attr_name, attr_value)?;
                    }
                }
                Event::End(e) => {
                    let name_bytes = e.name();
                    let name = std::str::from_utf8(name_bytes.as_ref())?;
                    serializer.end_tag(name)?;
                    tag_stack.pop();
                }
                Event::Empty(e) => {
                    let name_bytes = e.name();
                    let name = std::str::from_utf8(name_bytes.as_ref())?;
                    if name.contains(':') {
                        show_warning(
                            "Namespaces and prefixes",
                            Some(&format!("Found prefixed element: {}", name)),
                        );
                    }

                    serializer.start_tag(name)?;
                    for attr in e.attributes() {
                        let attr = attr?;
                        let attr_name = std::str::from_utf8(attr.key.as_ref())?;
                        let attr_value = std::str::from_utf8(&attr.value)?;
                        if attr_name.starts_with("xmlns") || attr_name.contains(':') {
                            show_warning(
                                "Namespaces and prefixes",
                                Some(&format!(
                                    "Found namespace declaration or prefixed attribute: {}",
                                    attr_name
                                )),
                            );
                        }

                        Self::write_attribute(&mut serializer, attr_name, attr_value)?;
                    }

                    serializer.end_tag(name)?;
                }
                Event::Text(e) => {
                    let text = std::str::from_utf8(&e)?;
                    if type_detection::is_whitespace_only(text) {
                        if serializer.preserve_whitespace {
                            serializer.ignorable_whitespace(text)?;
                        }
                    } else {
                        serializer.text(text)?;
                    }
                }
                Event::CData(e) => {
                    let text = std::str::from_utf8(&e)?;
                    serializer.cdsect(text)?;
                }
                Event::Comment(e) => {
                    let text = std::str::from_utf8(&e)?;
                    serializer.comment(text)?;
                }
                Event::PI(e) => {
                    let target = std::str::from_utf8(e.target())?;
                    let raw = e.content();
                    let data = if raw.is_empty() {
                        None
                    } else {
                        Some(std::str::from_utf8(raw)?)
                    };

                    if target == "xml" {
                        if let Some(content) = data {
                            if content.contains("encoding")
                                && !content.to_lowercase().contains("utf-8")
                            {
                                show_warning(
                                    "Non‑UTF‑8 encoding",
                                    Some(&format!("Found in declaration: {}", content)),
                                );
                            }
                        }
                    }

                    serializer.processing_instruction(target, data)?;
                }
                Event::Decl(decl) => {
                    if let Some(enc_result) = decl.encoding() {
                        let enc_bytes = enc_result?;
                        let enc = std::str::from_utf8(enc_bytes.as_ref())?;
                        if !enc.to_lowercase().contains("utf-8") {
                            show_warning(
                                "Non-UTF-8 encoding",
                                Some(&format!("Found encoding: {}", enc)),
                            );
                        }
                    }
                }
                Event::DocType(e) => {
                    let text = std::str::from_utf8(&e)?;
                    serializer.docdecl(text)?;
                }
                Event::GeneralRef(e) => {
                    let text = std::str::from_utf8(&e)?;
                    serializer.entity_ref(text)?;
                }
                Event::Eof => break,
            }
            buf.clear();
        }

        serializer.end_document()?;
        Ok(())
    }

    fn write_attribute<W: Write>(
        serializer: &mut BinaryXmlSerializer<W>,
        name: &str,
        value: &str,
    ) -> Result<(), ConversionError> {
        use type_detection::*;

        // only convert truly unambiguous cases
        if is_boolean(value) {
            // "true" or "false" -> boolean
            serializer.attribute_boolean(name, value == "true")?;
        } else if is_scientific_notation(value) {
            // scientific notation like "1.23e10" -> a double
            match value.parse::<f64>() {
                Ok(double_val) => {
                    serializer.attribute_double(name, double_val)?;
                }
                Err(_) => {
                    // if parsing fails, keep as string
                    serializer.attribute(name, value)?;
                }
            }
        } else {
            // everything else -> store as string
            // use interned strings for short values without spaces (optimization)
            if value.len() < 50 && !value.contains(' ') {
                serializer.attribute_interned(name, value)?;
            } else {
                serializer.attribute(name, value)?;
            }
        }
        Ok(())
    }
}
