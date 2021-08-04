fn main() {
    if std::env::var("TARGET")
        .unwrap_or_default()
        .contains("android")
    {
        cc::Build::new()
            .file("src/ifaces/ffi/android/ifaddrs.cpp")
            .compile("ifaddrs-android");
    }
}
