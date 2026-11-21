//! Android: Oboe (the audio backend of cpal) is C++, and oboe-sys asks for the static C++ runtime before its own
//! library on the linker line, so its C++ symbols stay unresolved (`__cxa_pure_virtual`, `operator new`) and the
//! APK dies at start with "dlopen failed: cannot locate symbol". Linking the runtime once more at the very end fixes it.
fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("android") {
        println!("cargo:rustc-link-arg=-lc++_static");
        println!("cargo:rustc-link-arg=-lc++abi");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
