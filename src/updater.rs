//! Self-update from a public GitHub repo (like the AOS launcher): `update_url.txt` next to the exe holds the raw base URL
//! (https://raw.githubusercontent.com/USER/REPO/main/), which serves `latest.json` ({"build": <unix secs>}) and `VinteriaDaw.exe`.
//! A newer build is downloaded over `VinteriaDaw.exe`; the running copy notices the new file and restarts itself (see `check_update`).
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone, PartialEq, Debug)]
pub enum UpState {
    Idle,
    Checking,
    Available(u64),
    Latest,
    Downloading,
    Done,
    Error(String),
    NoRepo,
}

#[derive(Clone)]
pub struct Updater {
    pub state: Arc<Mutex<UpState>>,
}

fn base() -> Option<String> {
    let mut dirs: Vec<PathBuf> = vec![PathBuf::from(".")];
    if let Ok(e) = std::env::current_exe() {
        if let Some(d) = e.parent() {
            dirs.push(d.to_path_buf());
            if let Some(dd) = d.parent() {
                dirs.push(dd.to_path_buf());
            }
        }
    }
    for d in dirs {
        if let Ok(t) = std::fs::read_to_string(d.join("update_url.txt")) {
            let t = t.trim().trim_end_matches('/').to_string();
            if t.starts_with("http") {
                return Some(t);
            }
        }
    }
    None
}

fn curl() -> std::process::Command {
    let mut c = std::process::Command::new("curl.exe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // no console window
    }
    c
}

fn mtime_secs(p: &Path) -> u64 {
    std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}

impl Updater {
    pub fn new() -> Self {
        Updater { state: Arc::new(Mutex::new(UpState::Idle)) }
    }

    pub fn get(&self) -> UpState {
        self.state.lock().map(|s| s.clone()).unwrap_or(UpState::Idle)
    }

    fn set(st: &Arc<Mutex<UpState>>, v: UpState) {
        if let Ok(mut s) = st.lock() {
            *s = v;
        }
    }

    /// Ask GitHub for `latest.json` and compare its build with the exe at `orig`.
    pub fn check(&self, orig: PathBuf) {
        let st = Arc::clone(&self.state);
        let Some(url) = base() else {
            Self::set(&st, UpState::NoRepo);
            return;
        };
        Self::set(&st, UpState::Checking);
        std::thread::spawn(move || {
            let out = curl().args(["-sL", "--max-time", "15", "-H", "Cache-Control: no-cache", &format!("{url}/latest.json")]).output();
            let text = match out {
                Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
                _ => return Self::set(&st, UpState::Error("no connection".into())),
            };
            let build = text.split("\"build\"").nth(1).and_then(|r| r.trim_start_matches([' ', ':']).split(|c: char| !c.is_ascii_digit()).next().map(|s| s.to_string())).and_then(|s| s.parse::<u64>().ok());
            match build {
                Some(b) if b > mtime_secs(&orig) + 5 => Self::set(&st, UpState::Available(b)),
                Some(_) => Self::set(&st, UpState::Latest),
                None => Self::set(&st, UpState::Error("latest.json is not readable".into())),
            }
        });
    }

    /// Download the new exe over `orig`; the running copy restarts into it by itself.
    pub fn download(&self, orig: PathBuf) {
        let st = Arc::clone(&self.state);
        let Some(url) = base() else {
            Self::set(&st, UpState::NoRepo);
            return;
        };
        Self::set(&st, UpState::Downloading);
        std::thread::spawn(move || {
            let tmp = orig.with_extension("download");
            let ok = curl().args(["-sL", "--max-time", "300", "-o"]).arg(&tmp).arg(format!("{url}/VinteriaDaw.exe")).status().map_or(false, |s| s.success());
            let big = std::fs::metadata(&tmp).map_or(false, |m| m.len() > 1_000_000);
            if !(ok && big) {
                let _ = std::fs::remove_file(&tmp);
                return Self::set(&st, UpState::Error("download failed".into()));
            }
            match std::fs::rename(&tmp, &orig) {
                Ok(_) => Self::set(&st, UpState::Done),
                Err(e) => Self::set(&st, UpState::Error(format!("cannot replace the exe: {e}"))),
            }
        });
    }
}
