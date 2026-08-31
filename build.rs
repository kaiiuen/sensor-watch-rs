fn main() {
    if std::env::var_os("CARGO_FEATURE_MINIMAL_USB").is_some() {
        println!(
            "cargo:warning=DEVELOPER ONLY: minimal-usb replaces the normal application and implements bounded CDC contracts with hardware bulk transfer fail-closed"
        );
        println!(
            "cargo:warning=DEVELOPER ONLY: CDC bulk hardware remains disabled until SRAM/endpoint behavior is proven"
        );
    }
}
