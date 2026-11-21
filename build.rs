fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        println!("cargo:rustc-link-arg-cdylib=-Wl,-z,max-page-size=16384");
        // CPAL's Oboe backend needs the NDK C++ runtime inside this library.
        println!("cargo:rustc-link-arg-cdylib=-lc++_static");
        println!("cargo:rustc-link-arg-cdylib=-lc++abi");
        println!("cargo:rustc-link-arg-cdylib=-Wl,--no-undefined");
    }
}
