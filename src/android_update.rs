use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use android_activity::AndroidApp;
use jni::objects::{JByteArray, JObject, JValue};
use jni::{JNIEnv, JavaVM};
use serde::Deserialize;

const FEED: &str = "https://raw.githubusercontent.com/dozyFJ/winteria-updates/main/";
const CHECK_INTERVAL: Duration = Duration::from_secs(600);
const MAX_MANIFEST_BYTES: usize = 16 * 1024;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Release {
    version_code: i32,
    version_name: String,
    url: String,
    sha256: String,
}

impl Release {
    fn valid(&self) -> bool {
        !self.version_name.is_empty()
            && self.version_name.len() <= 32
            && self.version_name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-')
            && self.url == format!("{FEED}releases/Winteria-{}.apk", self.version_name)
            && self.sha256.len() == 64
            && self.sha256.bytes().all(|c| c.is_ascii_hexdigit())
    }
}

#[derive(Clone)]
enum Status {
    Idle,
    Checking,
    Current,
    Available(Release),
    Failed(&'static str),
}

pub(crate) struct Updater {
    app: AndroidApp,
    status: Arc<Mutex<Status>>,
    last_check: Option<Instant>,
}

impl Updater {
    pub(crate) fn new(app: AndroidApp) -> Self {
        Self {
            app,
            status: Arc::new(Mutex::new(Status::Idle)),
            last_check: None,
        }
    }

    fn check(&mut self, ctx: &egui::Context) {
        self.last_check = Some(Instant::now());
        *self.status.lock().unwrap() = Status::Checking;
        let status = Arc::clone(&self.status);
        let app = self.app.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = with_activity(&app, |env, activity| {
                let installed = installed_version(env, activity)?;
                let bytes = read_manifest(env)?;
                Ok((installed, bytes))
            });
            *status.lock().unwrap() = match result {
                Ok((installed, bytes)) => match serde_json::from_slice::<Release>(&bytes) {
                    Ok(release) if release.valid() && release.version_code > installed => Status::Available(release),
                    Ok(release) if release.valid() => Status::Current,
                    _ => Status::Failed("Invalid update manifest. Tap to retry."),
                },
                Err(_) => Status::Failed("Could not check for updates. Tap to retry."),
            };
            ctx.request_repaint();
        });
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui) {
        let status = self.status.lock().unwrap().clone();
        let elapsed = self.last_check.map_or(CHECK_INTERVAL, |last| last.elapsed());
        if elapsed >= CHECK_INTERVAL && !matches!(status, Status::Checking) {
            self.check(ui.ctx());
            ui.ctx().request_repaint_after(CHECK_INTERVAL);
            return;
        }
        ui.ctx().request_repaint_after(CHECK_INTERVAL.saturating_sub(elapsed));
        match status {
            Status::Available(release) => {
                if ui.button(format!("Download {}", release.version_name))
                    .on_hover_text("Opens the APK in your browser. Confirm installation in Android.")
                    .clicked()
                    && open_browser(&self.app, &release.url).is_err()
                {
                    *self.status.lock().unwrap() = Status::Failed("Could not open the browser. Tap to retry.");
                }
            }
            Status::Idle | Status::Current | Status::Failed(_) => {
                let label = match status {
                    Status::Failed(message) => message,
                    Status::Current => "Up to date · Check again",
                    _ => "Check for updates",
                };
                if ui.button(label).clicked() {
                    self.check(ui.ctx());
                }
            }
            Status::Checking => {
                ui.spinner();
                ui.label("Checking for updates");
            }
        }
    }
}

fn with_activity<T>(
    app: &AndroidApp,
    f: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> jni::errors::Result<T>,
) -> jni::errors::Result<T> {
    // AndroidApp keeps the VM and its global Activity reference alive for this call.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let result = env.with_local_frame(32, |env| f(env, &activity));
    if env.exception_check()? {
        env.exception_clear()?;
    }
    result
}

fn installed_version(env: &mut JNIEnv<'_>, activity: &JObject<'_>) -> jni::errors::Result<i32> {
    let manager = env.call_method(activity, "getPackageManager", "()Landroid/content/pm/PackageManager;", &[])?.l()?;
    let package = env.call_method(activity, "getPackageName", "()Ljava/lang/String;", &[])?.l()?;
    let info = env.call_method(
        manager, "getPackageInfo", "(Ljava/lang/String;I)Landroid/content/pm/PackageInfo;",
        &[JValue::Object(&package), JValue::Int(0)],
    )?.l()?;
    env.get_field(info, "versionCode", "I")?.i()
}

fn read_manifest(env: &mut JNIEnv<'_>) -> jni::errors::Result<Vec<u8>> {
    let address = env.new_string(format!("{FEED}latest.json"))?;
    let url = env.new_object("java/net/URL", "(Ljava/lang/String;)V", &[JValue::Object(&address)])?;
    let connection = env.call_method(url, "openConnection", "()Ljava/net/URLConnection;", &[])?.l()?;
    for method in ["setConnectTimeout", "setReadTimeout"] {
        env.call_method(&connection, method, "(I)V", &[JValue::Int(8000)])?;
    }
    env.call_method(&connection, "setUseCaches", "(Z)V", &[JValue::Bool(0)])?;
    let input = env.call_method(&connection, "getInputStream", "()Ljava/io/InputStream;", &[])?.l()?;
    let result = read_stream(env, &input);
    if env.exception_check()? {
        env.exception_clear()?;
    }
    let _ = env.call_method(input, "close", "()V", &[]);
    result
}

fn read_stream(env: &mut JNIEnv<'_>, input: &JObject<'_>) -> jni::errors::Result<Vec<u8>> {
    let buffer: JByteArray<'_> = env.new_byte_array(2048)?;
    let mut bytes = Vec::new();
    while bytes.len() <= MAX_MANIFEST_BYTES {
        let count = env.call_method(input, "read", "([B)I", &[JValue::Object(&buffer)])?.i()?;
        if count <= 0 {
            break;
        }
        bytes.extend_from_slice(&env.convert_byte_array(&buffer)?[..count as usize]);
    }
    if bytes.len() > MAX_MANIFEST_BYTES {
        bytes.clear();
    }
    Ok(bytes)
}

fn open_browser(app: &AndroidApp, url: &str) -> jni::errors::Result<()> {
    with_activity(app, |env, activity| {
        let uri = env.new_string(url)?;
        let uri = env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&uri)])?.l()?;
        let action = env.new_string("android.intent.action.VIEW")?;
        let intent = env.new_object(
            "android/content/Intent", "(Ljava/lang/String;Landroid/net/Uri;)V",
            &[JValue::Object(&action), JValue::Object(&uri)],
        )?;
        env.call_method(activity, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
        Ok(())
    })
}
