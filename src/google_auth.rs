//! Google sign-in for the profile button: OAuth "installed app" flow with a loopback redirect.
//! The OAuth client (client_id / client_secret of a "Desktop app" in Google Cloud Console) lives in `google_oauth.json`;
//! only the name and e-mail are kept (`google_profile.json`), never a token. Token and user-info requests go through
//! `curl.exe` (shipped with Windows 10+), so no TLS stack is needed here.
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::Mutex;

const CONFIG: &str = "google_oauth.json";
const PROFILE: &str = "google_profile.json";

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub email: String,
}

#[derive(Clone, PartialEq)]
pub enum Status {
    SignedOut,
    Waiting,
    Failed(String),
}

#[derive(Deserialize)]
struct Config {
    client_id: String,
    client_secret: String,
}

struct State {
    profile: Option<Profile>,
    status: Status,
    loaded: bool,
}

static STATE: Mutex<State> = Mutex::new(State { profile: None, status: Status::SignedOut, loaded: false });

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if !g.loaded {
        g.loaded = true;
        g.profile = std::fs::read_to_string(PROFILE).ok().and_then(|t| serde_json::from_str(&t).ok());
    }
    f(&mut g)
}

pub fn profile() -> Option<Profile> {
    with(|s| s.profile.clone())
}

pub fn status() -> Status {
    with(|s| s.status.clone())
}

pub fn configured() -> bool {
    load_config().is_ok()
}

/// Writes a template `google_oauth.json` (when missing) so the user only has to fill the two values in.
pub fn create_config_template() -> std::path::PathBuf {
    let path = std::path::PathBuf::from(CONFIG);
    if !path.exists() {
        let _ = std::fs::write(&path, "{\n  \"client_id\": \"\",\n  \"client_secret\": \"\"\n}\n");
    }
    std::fs::canonicalize(&path).unwrap_or(path)
}

pub fn sign_out() {
    let _ = std::fs::remove_file(PROFILE);
    with(|s| {
        s.profile = None;
        s.status = Status::SignedOut;
    });
}

fn load_config() -> Result<Config, String> {
    let text = std::fs::read_to_string(CONFIG).map_err(|_| format!("{CONFIG} not found"))?;
    let c: Config = serde_json::from_str(&text).map_err(|e| format!("{CONFIG}: {e}"))?;
    if c.client_id.trim().is_empty() || c.client_secret.trim().is_empty() {
        return Err(format!("fill client_id and client_secret in {CONFIG}"));
    }
    Ok(c)
}

fn enc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

fn dec(s: &str) -> String {
    let b = s.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn nonce() -> String {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(1);
    format!("{:x}{:x}", t, std::process::id())
}

fn curl(args: &[&str]) -> Result<serde_json::Value, String> {
    let out = Command::new("curl.exe").arg("-sS").args(args).output().map_err(|e| format!("curl.exe: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("bad reply: {e}"))
}

/// Starts the browser sign-in in the background; `status()` shows Waiting until it ends.
pub fn sign_in() {
    let cfg = match load_config() {
        Ok(c) => c,
        Err(e) => {
            with(|s| s.status = Status::Failed(e));
            return;
        }
    };
    if status() == Status::Waiting {
        return;
    }
    with(|s| s.status = Status::Waiting);
    std::thread::spawn(move || {
        let r = run(&cfg);
        with(|s| match r {
            Ok(p) => {
                let _ = std::fs::write(PROFILE, serde_json::to_string(&p).unwrap_or_default());
                s.profile = Some(p);
                s.status = Status::SignedOut;
            }
            Err(e) => s.status = Status::Failed(e),
        });
    });
}

fn run(cfg: &Config) -> Result<Profile, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}");
    let state = nonce();
    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}&prompt=select_account",
        enc(&cfg.client_id),
        enc(&redirect),
        enc("openid email profile"),
        state
    );
    Command::new("rundll32").args(["url.dll,FileProtocolHandler", &url]).spawn().map_err(|e| format!("could not open the browser: {e}"))?;
    // give up after 3 minutes
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let started = std::time::Instant::now();
    let (mut stream, _) = loop {
        match listener.accept() {
            Ok(c) => break c,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if started.elapsed().as_secs() > 180 {
                    return Err("sign-in timed out".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    stream.set_nonblocking(false).ok();
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).ok();
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
    let req = String::from_utf8_lossy(&buf[..n]).to_string();
    let query = req.lines().next().and_then(|l| l.split_whitespace().nth(1)).and_then(|p| p.split_once('?')).map(|x| x.1.to_string()).unwrap_or_default();
    let get = |k: &str| query.split('&').filter_map(|kv| kv.split_once('=')).find(|(a, _)| *a == k).map(|(_, v)| dec(v));
    let result = (|| {
        if let Some(e) = get("error") {
            return Err(format!("Google: {e}"));
        }
        if get("state").as_deref() != Some(state.as_str()) {
            return Err("sign-in answer did not match (state)".into());
        }
        let code = get("code").ok_or("no code in the answer")?;
        let tok = curl(&[
            "https://oauth2.googleapis.com/token",
            "-d", &format!("code={}", enc(&code)),
            "-d", &format!("client_id={}", enc(&cfg.client_id)),
            "-d", &format!("client_secret={}", enc(&cfg.client_secret)),
            "-d", &format!("redirect_uri={}", enc(&redirect)),
            "-d", "grant_type=authorization_code",
        ])?;
        let access = tok["access_token"].as_str().ok_or_else(|| tok["error_description"].as_str().unwrap_or("no access token").to_string())?.to_string();
        let info = curl(&["https://openidconnect.googleapis.com/v1/userinfo", "-H", &format!("Authorization: Bearer {access}")])?;
        Ok(Profile {
            name: info["name"].as_str().unwrap_or("").to_string(),
            email: info["email"].as_str().unwrap_or("").to_string(),
        })
    })();
    let body = if result.is_ok() { "Signed in. You can close this tab and return to Vinteria." } else { "Sign-in failed. Return to Vinteria for details." };
    let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<html><body style=\"font-family:sans-serif;padding:40px\"><h2>{body}</h2></body></html>");
    result
}
