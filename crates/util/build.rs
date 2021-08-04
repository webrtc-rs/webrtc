fn main() {
    if std::env::var("TARGET").unwrap_or(String::new()).contains("android") {
        cc::Build::new()
            .file("ifaddrs-android/ifaddrs-android.cpp")
            .compile("ifaddrs-android");
    }
}