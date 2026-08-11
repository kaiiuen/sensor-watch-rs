//! A small host tool that converts a raw firmware `.bin` into a `.uf2` file.
//!
//! Usage: `cargo run -p sensor-watch-core --bin uf2tool -- <input.bin> <output.uf2>`
//!
//! This is used to produce the drag-and-drop firmware artifact for the
//! Sensor Watch bootloader.

use sensor_watch_core::uf2::{MAX_APPLICATION_BYTES, convert_to_uf2};
use std::{fs, io::Read};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: uf2tool <input.bin> <output.uf2>");
        std::process::exit(1);
    }
    let input = &args[1];
    let output = &args[2];

    let metadata = match fs::symlink_metadata(input) {
        Ok(metadata) => metadata,
        Err(error) => {
            eprintln!("failed to inspect input binary: {error}");
            std::process::exit(1);
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        eprintln!("refusing non-regular input binary: {input}");
        std::process::exit(1);
    }
    let mut file = match fs::File::open(input) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("failed to read input binary: {error}");
            std::process::exit(1);
        }
    };
    let mut image = Vec::new();
    if let Err(error) = file
        .by_ref()
        .take(MAX_APPLICATION_BYTES as u64 + 1)
        .read_to_end(&mut image)
    {
        eprintln!("failed to read input binary: {error}");
        std::process::exit(1);
    }
    if image.len() > MAX_APPLICATION_BYTES {
        eprintln!("input binary exceeds the maximum application size");
        std::process::exit(1);
    }
    let uf2 = convert_to_uf2(&image);
    if uf2.is_empty() {
        eprintln!("input binary is empty or exceeds the maximum application size");
        std::process::exit(1);
    }
    if output != input
        && fs::symlink_metadata(output)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
    {
        eprintln!("refusing symlinked output path: {output}");
        std::process::exit(1);
    }
    if let Err(error) = fs::write(output, &uf2) {
        eprintln!("failed to write output uf2: {error}");
        std::process::exit(1);
    }
    println!("wrote {} bytes to {}", uf2.len(), output);
}
