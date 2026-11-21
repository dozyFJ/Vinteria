# Winteria DAW

DAW written from scratch in Rust: own audio engine, synths, step sequencer, piano roll, arrangement, mixer, project files, WAV export.

The Rust crate is `vinteria_daw`; the desktop executable is `VinteriaDaw.exe`.
Existing `.synthesia` projects remain compatible.

## Project layout
- `src/audio_engine/`: realtime playback, audio devices and WAV export.
- `src/instruments/`: synths, drums and sampler.
- `src/project.rs`: project persistence, patterns, scenes and sample loading.
- `src/arrangement.rs`, `src/session.rs`: timeline clips and clip launcher.
- `src/ui/`: shared desktop and Android interface, editors and shared theme.

## Stack (and why)
The machine has no MSVC / g++ / CMake / JUCE, so C++/JUCE was not possible. Chosen instead:
Rust (gnu toolchain) + `cpal` (WASAPI audio) + `egui/eframe` (UI) + `hound` (WAV) + `midir` (MIDI in).
VST3 hosting is **not implemented** (needs the VST3 SDK + a C++ toolchain) - planned for the next stage.

## Build & run (Windows)
```
set PATH=%USERPROFILE%\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained;%PATH%
cargo run --release            :: start the DAW (run from this folder; samples\ is created here)
cargo test                     :: unit tests (engine, export round-trip, save/load)
cargo run --release -- --play-test   :: headless test through the real audio device
```
`Cargo.lock` pins crates that would otherwise need a full MinGW `dlltool`; `vendor/webbrowser` is a no-op stub for the same reason.

## Build (Android)

Install Android SDK Platform 34, Build Tools 34.0.0, NDK r26d, and JDK. Set
`ANDROID_HOME` and `ANDROID_NDK_HOME` to the SDK and NDK directories.

```sh
rustup target add aarch64-linux-android
cargo install cargo-apk --version 0.10.0 --locked
cargo apk build --lib
```

The APK is at `target/debug/apk/Winteria.apk` and targets 64-bit ARM devices
running Android 8.0 or later. For a release build, set
`CARGO_APK_RELEASE_KEYSTORE` and `CARGO_APK_RELEASE_KEYSTORE_PASSWORD` and add
`--release`.

The Android app uses package ID `com.winteria.daw`. Keep the same signing
key for every release so Android can install updates over the existing app.

Android checks the public OTA feed on startup and every ten minutes while open.
When an update is available, the download button opens the APK in the system
browser. Android verifies the app signature and asks for installation approval.
To publish a release, increase the Cargo patch version, sign the APK, and publish
`releases/Winteria-<versionName>.apk` with `latest.json` on the `winteria` branch of `dozyFJ/pulse-updates`.
The manifest contains `versionCode`, `versionName`, `url`, and `sha256`.
Read the version code from the signed APK with `aapt dump badging`.

Android runs the desktop interface in landscape with audio playback via Oboe.
Projects and WAV exports are saved in app storage. External file selection,
microphone recording, and MIDI input remain desktop features.

## Keys
Desktop starts maximized. The horizontal navigation below the header opens
Browse, Playlist, Channel Rack, Piano Roll, Mixer, Session and Plugins. Browser categories wrap into rows above their vertically scrolling content;
click Browse again to collapse the panel. Piano Roll opens with F7; Mixer with F9.
Theme switches at the
navigation row. The look follows the AOS launcher: graphite surfaces, cyan accent, rounded pill controls (AOS Night / AOS Day).

Device cards wrap to the available width and scroll vertically. The chain includes the selected clip's options, followed by the track's
instrument and effects. MIDI clips expose instrument, WAV sound, Piano Roll,
transpose and conversion actions; audio clips expose sample replacement,
gain, reverse, fades and pitch. The same controls remain available on right-click.

Space play/pause; Stop rewinds. Song playback stops at the end when Loop is off.
Z..M lower piano row, Q..P upper piano row (number keys are sharps) - minus/equals change octave - Ctrl+S/O/N/E save/open/new/export.

## Update: themes, plugins, Smart Steps
- Themes: **Dark Graphite** and **Winter Light** (status bar > Theme). **UI Scale** 50-150 % in the same menu; stored in `theme.txt` (`v3,...`; older files still load).
- Plugin Grid Template (`src/ui/plugin_ui.rs`): one adaptive editor (header, presets, graph, knob grid, dry/wet) for every built-in plugin.
- 20 Essentials FX + signature plugins **ViHalla** (12 modes, Shimmer, Freeze, Ducking, Dual Space A/B) and **ViGross** (Reverse, Stutter, volume pattern, Pattern Morph) in `src/fx.rs` / `src/fx_ext.rs`. Old projects load unchanged (`FxSlot::x` defaults to zeros).
- Smart Steps (`src/ui/smart_steps.rs`): per-step Velocity / Pitch / Pan / Slide / Probability for drum lanes, Step Paint, waveform, Morph A/B. Stored in `sequencer::Step` (serde defaults keep old projects valid).
