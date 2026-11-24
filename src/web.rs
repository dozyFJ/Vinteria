//! Browser entry point and project storage for the shared Winteria application.
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wasm_bindgen::prelude::*;

const PROJECT_PREFIX: &str = "winteria:project:";
const TIME_PREFIX: &str = "winteria:saved:";

#[wasm_bindgen]
pub async fn start_winteria(canvas: web_sys::HtmlCanvasElement) -> Result<(), JsValue> {
    eframe::WebRunner::new()
        .start(canvas, eframe::WebOptions::default(), Box::new(|cc| {
            CONTEXT.with_borrow_mut(|context| *context = Some(cc.egui_ctx.clone()));
            Ok(Box::new(crate::ui::VinteriaApp::new(cc)))
        }))
        .await
}

fn storage() -> Result<web_sys::Storage, String> {
    web_sys::window().ok_or("Browser window unavailable")?
        .local_storage().map_err(|_| "Browser storage access denied")?
        .ok_or_else(|| "Browser storage unavailable".into())
}

pub fn save_project(path: &Path, text: &str) -> Result<(), String> {
    let store = storage()?;
    store.set_item(&format!("{PROJECT_PREFIX}{}", path.display()), text)
        .map_err(|_| "Could not save project. Browser storage might be full.".to_owned())?;
    let _ = store.set_item(&format!("{TIME_PREFIX}{}", path.display()), &js_sys::Date::now().to_string());
    Ok(())
}

pub fn read_project(path: &Path) -> Result<String, String> {
    storage()?.get_item(&format!("{PROJECT_PREFIX}{}", path.display()))
        .map_err(|_| "Could not read project".to_owned())?
        .ok_or_else(|| "Project not found".into())
}

pub fn projects(dir: &Path) -> Vec<(PathBuf, SystemTime, u64)> {
    let Ok(store) = storage() else { return Vec::new() };
    let mut projects = Vec::new();
    for index in 0..store.length().unwrap_or(0) {
        let Ok(Some(key)) = store.key(index) else { continue };
        let Some(name) = key.strip_prefix(PROJECT_PREFIX) else { continue };
        let path = PathBuf::from(name);
        if path.parent() != Some(dir) { continue; }
        let bytes = store.get_item(&key).ok().flatten().map_or(0, |value| value.len() as u64);
        let saved = store.get_item(&format!("{TIME_PREFIX}{name}")).ok().flatten()
            .and_then(|value| value.parse::<u64>().ok()).unwrap_or(0);
        projects.push((path, UNIX_EPOCH + Duration::from_millis(saved), bytes));
    }
    projects.sort_by_key(|(_, saved, _)| std::cmp::Reverse(*saved));
    projects
}

use std::{cell::{Cell, RefCell}, collections::BTreeMap, sync::Arc};

thread_local! {
    static SAMPLES: RefCell<BTreeMap<PathBuf, Arc<[u8]>>> = RefCell::new(BTreeMap::new());
    static SAMPLES_CHANGED: Cell<bool> = const { Cell::new(false) };
    static VOICE: RefCell<Option<Result<PathBuf, String>>> = const { RefCell::new(None) };
    static CONTEXT: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window, js_name = winteriaChooseSounds)]
    pub fn choose_sounds(folder: bool);
    #[wasm_bindgen(js_namespace = window, js_name = winteriaRecordVoice)]
    pub fn record_voice(start: bool);
    #[wasm_bindgen(js_namespace = window, js_name = winteriaDownloadWav)]
    fn download_wav(name: &str, bytes: &[u8]);
}

#[wasm_bindgen]
pub fn load_sample(relative_path: &str, bytes: Vec<u8>, validate: bool) -> Result<(), JsValue> {
    let path = Path::new(relative_path);
    if path.components().any(|part| !matches!(part, std::path::Component::Normal(_))) {
        return Err("Invalid WAV path".into());
    }
    if validate {
        crate::project::decode_wav(std::io::Cursor::new(bytes.as_slice()))
            .map_err(|error| JsValue::from_str(&error))?;
    }
    SAMPLES.with_borrow_mut(|samples| {
        samples.insert(Path::new("Browser samples").join(path), bytes.into());
    });
    SAMPLES_CHANGED.set(true);
    CONTEXT.with_borrow(|context| {
        if let Some(context) = context {
            context.request_repaint();
        }
    });
    Ok(())
}

pub fn sample_bytes(path: &Path) -> Option<Arc<[u8]>> {
    SAMPLES.with_borrow(|samples| samples.get(path).cloned())
}

pub fn take_sample_list() -> Option<Vec<PathBuf>> {
    SAMPLES_CHANGED.replace(false).then(|| SAMPLES.with_borrow(|samples| samples.keys().cloned().collect()))
}

#[wasm_bindgen]
pub fn voice_result(name: &str, error: Option<String>) {
    let result = error.map_or_else(|| Ok(Path::new("Browser samples").join(name)), Err);
    VOICE.with_borrow_mut(|voice| *voice = Some(result));
    CONTEXT.with_borrow(|context| {
        if let Some(context) = context {
            context.request_repaint();
        }
    });
}

pub fn take_voice_result() -> Option<Result<PathBuf, String>> {
    VOICE.with_borrow_mut(Option::take)
}

pub fn export_wav(project: &crate::project::Project, settings: &crate::audio_engine::export::ExportSettings, name: &str) -> Result<(), String> {
    use crate::audio_engine::export;
    download_rendered_wav(name, export::render(project, settings), settings)
}

pub fn export_wav_range(project: &crate::project::Project, settings: &crate::audio_engine::export::ExportSettings,
    name: &str, start: u32, end: u32) -> Result<(), String>
{
    download_rendered_wav(name, crate::audio_engine::export::render_range(project, settings, start, end)?, settings)
}

fn download_rendered_wav(name: &str, data: Vec<f32>, settings: &crate::audio_engine::export::ExportSettings) -> Result<(), String> {
    use crate::audio_engine::export;
    let mut bytes = Vec::new();
    export::write_wav_to(std::io::Cursor::new(&mut bytes), &data, settings)?;
    download_wav(name, &bytes);
    Ok(())
}
