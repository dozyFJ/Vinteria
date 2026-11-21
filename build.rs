//! Android linking. Oboe (the audio backend of cpal) is C++, and oboe-sys asks for the static C++ runtime before its own
//! library on the linker line, so its C++ symbols stayed unresolved (`__cxa_pure_virtual`, `operator new`) and the APK
//! died at start with "dlopen failed: cannot locate symbol". The runtime is linked once more at the very end, and
//! `--no-undefined` turns any such leftover into a build error instead of a crash on the phone.
//! 16 KB page alignment is required by newer Android devices.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        println!("cargo:rustc-link-arg-cdylib=-Wl,-z,max-page-size=16384");
        println!("cargo:rustc-link-arg-cdylib=-lc++_static");
        println!("cargo:rustc-link-arg-cdylib=-lc++abi");
        println!("cargo:rustc-link-arg-cdylib=-Wl,--no-undefined");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
