use clap::{Arg, Command};
use std::fs::File;
use std::io::Read;
use std::io::{self, BufWriter};
use xml2abx::XmlToAbxConverter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("xml2abx")
        .arg(
            Arg::new("input")
                .help("Input XML file (use '-' for stdin)")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("output")
                .help("Output ABX file (use '-' for stdout)")
                .index(2),
        )
        .arg(
            Arg::new("in-place")
                .long("in-place")
                .short('i')
                .help("Overwrite the input file with the output")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("collapse-whitespace")
                .long("collapse-whitespace")
                .help("Collapse whitespace")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let input_path = matches.get_one::<String>("input").unwrap();
    let output_path = matches.get_one::<String>("output");
    let in_place = matches.get_flag("in-place");
    let collapse_whitespace = matches.get_flag("collapse-whitespace");
    
    // preserve_whitespace is the inverse of collapse_whitespace
    let preserve_whitespace = !collapse_whitespace;

    let final_output_path = if in_place {
        if input_path == "-" {
            eprintln!("Error: Cannot overwrite stdin, output path is required");
            std::process::exit(1);
        }
        Some(input_path.clone())
    } else if let Some(output) = output_path {
        Some(output.clone())
    } else {
        eprintln!("Error: Output path is required (use '-' for stdout or specify a file)");
        std::process::exit(1);
    };

    let result = if input_path == "-" {
        let mut xml_content = String::new();
        io::stdin().read_to_string(&mut xml_content)?;

        if let Some(ref output_path) = final_output_path {
            if output_path == "-" {
                XmlToAbxConverter::convert_from_string_with_options(&xml_content, io::stdout(), preserve_whitespace)
            } else {
                let file = File::create(output_path)?;
                let writer = BufWriter::new(file);
                XmlToAbxConverter::convert_from_string_with_options(&xml_content, writer, preserve_whitespace)
            }
        } else {
            eprintln!("Error: Output path is required");
            std::process::exit(1);
        }
    } else {
        // for in-place editing, we need to read the file completely first

        let xml_content = std::fs::read_to_string(input_path)?;

        if let Some(ref output_path) = final_output_path {
            if output_path == "-" {
                XmlToAbxConverter::convert_from_string_with_options(&xml_content, io::stdout(), preserve_whitespace)
            } else {
                let file = File::create(output_path)?;
                let writer = BufWriter::new(file);
                XmlToAbxConverter::convert_from_string_with_options(&xml_content, writer, preserve_whitespace)
            }
        } else {
            eprintln!("Error: Output path is required");
            std::process::exit(1);
        }
    };

    match result {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}