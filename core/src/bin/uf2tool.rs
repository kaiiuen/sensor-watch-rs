//! A small host tool that converts a raw firmware `.bin` into a `.uf2` file.
//!
//! Usage: `cargo run -p sensor-watch-core --bin uf2tool -- <input.bin> <output.uf2>`
//!
//! This is used to produce the drag-and-drop firmware artifact for the
//! Sensor Watch bootloader.

use sensor_watch_core::uf2::convert_to_uf2;
use std::fs;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: uf2tool <input.bin> <output.uf2>");
        std::process::exit(1);
    }
    let input = &args[1];
    let output = &args[2];

    let image = match fs::read(input) {
        Ok(image) => image,
        Err(error) => {
            eprintln!("failed to read input binary: {error}");
            std::process::exit(1);
        }
    };
    let uf2 = convert_to_uf2(&image);
    if uf2.is_empty() {
        eprintln!("input binary is empty or exceeds the maximum application size");
        std::process::exit(1);
    }
    if let Err(error) = fs::write(output, &uf2) {
        eprintln!("failed to write output uf2: {error}");
        std::process::exit(1);
    }
    println!("wrote {} bytes to {}", uf2.len(), output);
}
