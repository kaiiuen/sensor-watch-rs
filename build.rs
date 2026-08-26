fn main() {
    if std::env::var_os("CARGO_FEATURE_MINIMAL_USB").is_some() {
        println!(
            "cargo:warning=DEVELOPER ONLY: minimal-usb replaces the normal application and does not implement USB"
        );
        println!(
            "cargo:warning=DEVELOPER ONLY: restore the production firmware after feasibility work"
        );
    }
}
