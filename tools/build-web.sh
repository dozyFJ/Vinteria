#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
target=${CARGO_TARGET_DIR:-target}
dist="$target/winteria-pwa"
cargo build --locked --lib --target wasm32-unknown-unknown --release
mkdir -p "$dist/pkg"
cp -R web/. "$dist/"
wasm-bindgen --target web --out-name winteria --out-dir "$dist/pkg" \
  "$target/wasm32-unknown-unknown/release/vinteria_daw.wasm"
version=$(cd "$dist" && find . -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -c1-16)
sed -i "s/BUILD_ID/$version/" "$dist/sw.js"
printf 'PWA files: %s\n' "$dist"
