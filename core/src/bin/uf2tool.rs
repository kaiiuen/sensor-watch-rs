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

    let image = fs::read(input).expect("failed to read input binary");
    let uf2 = convert_to_uf2(&image);
    fs::write(output, &uf2).expect("failed to write output uf2");
    println!("wrote {} bytes to {}", uf2.len(), output);
}
