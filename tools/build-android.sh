#!/usr/bin/env bash
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

profile=debug
build_args=(--lib)
case "${1:-}" in
    ""|--debug) ;;
    --release)
        profile=release
        build_args+=(--release)
        : "${CARGO_APK_RELEASE_KEYSTORE:?Set the path to the existing Android signing keystore}"
        : "${CARGO_APK_RELEASE_KEYSTORE_PASSWORD:?Provide the signing password through the environment}"
        if [[ ! -r "$CARGO_APK_RELEASE_KEYSTORE" ]]; then
            printf 'Cannot read the Android signing keystore.\n' >&2
            exit 1
        fi
        ;;
    *)
        printf 'Usage: %s [--debug|--release]\n' "$0" >&2
        exit 2
        ;;
esac

: "${ANDROID_HOME:?Set ANDROID_HOME to the Android SDK directory}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/26.3.11579264}"
apksigner="$ANDROID_HOME/build-tools/34.0.0/apksigner"
if [[ ! -x "$apksigner" || ! -f "$ANDROID_HOME/platforms/android-34/android.jar" || ! -f "$ANDROID_NDK_HOME/source.properties" ]]; then
    printf 'Install Android SDK Platform 34, Build Tools 34.0.0 and NDK r26d.\n' >&2
    exit 1
fi

target_dir="${CARGO_TARGET_DIR:-$PWD/target}"
cargo apk build "${build_args[@]}" --target-dir "$target_dir"
apk="$target_dir/$profile/apk/Winteria.apk"
"$apksigner" verify "$apk"
# the .so must not need C++ symbols that nothing provides (that is the "cannot locate symbol __cxa_pure_virtual" start-up crash)
so="$target_dir/aarch64-linux-android/$profile/libvinteria_daw.so"
nm_tool=$(ls "$ANDROID_NDK_HOME"/toolchains/llvm/prebuilt/*/bin/llvm-nm 2>/dev/null | head -n1)
if [[ -n "$nm_tool" && -f "$so" ]] && "$nm_tool" -D --undefined-only "$so" | grep -Eq ' (__cxa_pure_virtual|_Znwm|__cxa_begin_catch)$'; then
    printf 'libvinteria_daw.so has unresolved C++ symbols: the APK would crash at start.\n' >&2
    exit 1
fi
printf '%s\n' "$apk"
