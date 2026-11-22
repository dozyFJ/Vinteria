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
mkdir -p "$target_dir"
target_dir=$(realpath "$target_dir")
build_tools="$ANDROID_HOME/build-tools/34.0.0"
assembly=$(mktemp -d "$target_dir/.winteria-apk.XXXXXX")
trap 'rm -rf -- "$assembly"' EXIT
platform="$ANDROID_HOME/platforms/android-34/android.jar"
activity=android/java/com/winteria/daw/WinteriaActivity.java
mkdir "$assembly/classes"
if [[ -n "${WINTERIA_ECJ_JAR:-}" ]]; then
    java -jar "$WINTERIA_ECJ_JAR" -8 -proc:none -classpath "$platform" -d "$assembly/classes" "$activity"
else
    javac --release 8 -classpath "$platform" -d "$assembly/classes" "$activity"
fi
find "$assembly/classes" -name "*.class" -print0 | xargs -0 "$build_tools/d8" --min-api 26 --lib "$platform" --output "$assembly"

cargo apk build "${build_args[@]}" --target-dir "$target_dir"
apk="$target_dir/$profile/apk/Winteria.apk"
if [[ "$profile" == release ]]; then
    signing_key="$CARGO_APK_RELEASE_KEYSTORE"
    export WINTERIA_PACKAGING_PASSWORD="$CARGO_APK_RELEASE_KEYSTORE_PASSWORD"
else
    if [[ -n "${ANDROID_SDK_HOME:-}" ]]; then
        dev_home="$ANDROID_SDK_HOME/.android"
    else
        dev_home="${ANDROID_USER_HOME:-$HOME/.android}"
    fi
    signing_key="${CARGO_APK_DEV_KEYSTORE:-$dev_home/debug.keystore}"
    export WINTERIA_PACKAGING_PASSWORD="${CARGO_APK_DEV_KEYSTORE_PASSWORD:-android}"
fi
previous_signer=$("$apksigner" verify --print-certs "$apk" | sed -n 's/^Signer #1 certificate SHA-256 digest: //p')
cp "$apk" "$assembly/unaligned.apk"
(
    cd "$assembly"
    "$build_tools/aapt" add unaligned.apk classes.dex
)
"$build_tools/zipalign" -f -p 4 "$assembly/unaligned.apk" "$assembly/aligned.apk"
"$apksigner" sign --ks "$signing_key" --ks-pass env:WINTERIA_PACKAGING_PASSWORD     --out "$assembly/Winteria.apk" "$assembly/aligned.apk"
current_signer=$("$apksigner" verify --print-certs "$assembly/Winteria.apk" | sed -n 's/^Signer #1 certificate SHA-256 digest: //p')
if [[ -z "$previous_signer" || "$previous_signer" != "$current_signer" ]]; then
    printf 'APK packaging changed the signing certificate.\n' >&2
    exit 1
fi
mv "$assembly/Winteria.apk" "$apk"
# the manifest must let Android load classes.dex, or the Java launcher activity is missing at start
if ! "$build_tools/aapt" dump xmltree "$apk" AndroidManifest.xml | grep -q 'android:hasCode(0x0101000c)=(type 0x12)0xffffffff'; then
    printf 'AndroidManifest.xml has hasCode=false: the app would crash at start (ClassNotFoundException).\n' >&2
    exit 1
fi
# the .so must not need C++ symbols that nothing provides (that is the "cannot locate symbol __cxa_pure_virtual" start-up crash)
so="$target_dir/aarch64-linux-android/$profile/libvinteria_daw.so"
nm_tool=$(ls "$ANDROID_NDK_HOME"/toolchains/llvm/prebuilt/*/bin/llvm-nm 2>/dev/null | head -n1)
if [[ -n "$nm_tool" && -f "$so" ]] && "$nm_tool" -D --undefined-only "$so" | grep -Eq ' (__cxa_pure_virtual|_Znwm|__cxa_begin_catch)$'; then
    printf 'libvinteria_daw.so has unresolved C++ symbols: the APK would crash at start.\n' >&2
    exit 1
fi
printf '%s\n' "$apk"
