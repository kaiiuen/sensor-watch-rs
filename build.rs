fn main() {
    if std::env::var_os("CARGO_FEATURE_MINIMAL_USB").is_some() {
        println!(
            "cargo:warning=DEVELOPER ONLY: minimal-usb replaces the normal application and implements EP0 enumeration only"
        );
        println!("cargo:warning=DEVELOPER ONLY: no CDC bulk shell is included");
    }
}
