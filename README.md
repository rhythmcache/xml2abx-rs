# xml2abx-rs
Rust Library To Convert Human Readable XML into Android Binary Xml (ABX)


## CLI Installation
```bash
cargo install xml2abx
```

## CLI Usage
```bash
Usage: xml2abx [OPTIONS] <input> [output]

Arguments:
  <input>   Input XML file (use '-' for stdin)
  [output]  Output ABX file (use '-' for stdout)

Options:
  -i, --in-place  Overwrite the input file with the output
```

## Library Usage
- Basic Usage

```rust
use xml2abx::XmlToAbxConverter;

let xml = r#"<root><element attr="value">text</element></root>"#;
let mut output = Vec::new();
XmlToAbxConverter::convert_from_string(xml, &mut output)?;
```
- Convert from File

```rust
use xml2abx::XmlToAbxConverter;
use std::fs::File;

let mut output = File::create("output.abx")?;
XmlToAbxConverter::convert_from_file("input.xml", &mut output)?;
```

- Convert From Reader
```rust
use xml2abx::XmlToAbxConverter;
use std::io::Cursor;

let xml_data = b"<root><item>value</item></root>";
let reader = Cursor::new(xml_data);
let mut output = Vec::new();
XmlToAbxConverter::convert_from_reader(reader, &mut output)?;
```

```rust
// Use BufWriter for better performance
use std::io::BufWriter;
let writer = BufWriter::new(file);

// Reuse buffers for multiple conversions
let mut buffer = Vec::new();
for xml_doc in docs {
    buffer.clear();
    XmlToAbxConverter::convert_from_string(&xml_doc, &mut buffer)?;
}
```



### Sources
- [BinaryXmlSerializer.java](https://cs.android.com/android/platform/superproject/+/master:frameworks/base/core/java/com/android/internal/util/BinaryXmlSerializer.java;bpv=0)

- [abx2xml](https://github.com/rhythmcache/xml2abx-rs/edit/main/README.md)


### License
This project is licensed under
- Apache License, Version 2.0
