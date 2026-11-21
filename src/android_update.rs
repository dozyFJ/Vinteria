use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use android_activity::AndroidApp;
use jni::objects::{JByteArray, JObject, JValue};
use jni::{JNIEnv, JavaVM};
use serde::Deserialize;

const FEED: &str = "https://raw.githubusercontent.com/dozyFJ/pulse-updates/winteria/";
const CHECK_INTERVAL: Duration = Duration::from_secs(600);
const MAX_MANIFEST_BYTES: usize = 16 * 1024;
const MAX_APK_BYTES: usize = 64 * 1024 * 1024;
const APK_MIME: &str = "application/vnd.android.package-archive";

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
    Downloading,
    Downloaded(String),
    Ready(String, Option<String>),
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

    pub(crate) fn request_landscape(&self, landscape: bool) -> Result<(), String> {
        const SENSOR_LANDSCAPE: i32 = 6;
        const SENSOR_PORTRAIT: i32 = 7;
        let orientation = if landscape { SENSOR_LANDSCAPE } else { SENSOR_PORTRAIT };
        with_activity(&self.app, |env, activity| {
            env.call_method(activity, "setRequestedOrientation", "(I)V", &[JValue::Int(orientation)])?;
            Ok(())
        }).map_err(|error| error.to_string())
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
                let bytes = get_bytes(env, &format!("{FEED}latest.json"), MAX_MANIFEST_BYTES)?;
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

    /// Hand a file to Android's share sheet (Telegram, TikTok, Drive, ...).
    pub(crate) fn share_file(&self, path: &str, mime: &str) -> Result<(), String> {
        with_activity(&self.app, |env, activity| {
            // a file:// link would be refused by StrictMode on Android 7+: relax the VM policy for this app
            let builder = env.new_object("android/os/StrictMode$VmPolicy$Builder", "()V", &[])?;
            let policy = env.call_method(&builder, "build", "()Landroid/os/StrictMode$VmPolicy;", &[])?.l()?;
            env.call_static_method("android/os/StrictMode", "setVmPolicy", "(Landroid/os/StrictMode$VmPolicy;)V", &[JValue::Object(&policy)])?;
            let jpath = env.new_string(path)?;
            let file = env.new_object("java/io/File", "(Ljava/lang/String;)V", &[JValue::Object(&jpath)])?;
            let uri = env.call_static_method("android/net/Uri", "fromFile", "(Ljava/io/File;)Landroid/net/Uri;", &[JValue::Object(&file)])?.l()?;
            let action = env.new_string("android.intent.action.SEND")?;
            let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])?;
            let jmime = env.new_string(mime)?;
            env.call_method(&intent, "setType", "(Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&jmime)])?;
            let extra = env.new_string("android.intent.extra.STREAM")?;
            env.call_method(&intent, "putExtra", "(Ljava/lang/String;Landroid/os/Parcelable;)Landroid/content/Intent;", &[JValue::Object(&extra), JValue::Object(&uri)])?;
            env.call_method(&intent, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(1)])?;
            let title = env.new_string("Share")?;
            let chooser = env.call_static_method(
                "android/content/Intent", "createChooser",
                "(Landroid/content/Intent;Ljava/lang/CharSequence;)Landroid/content/Intent;",
                &[JValue::Object(&intent), JValue::Object(&title)],
            )?.l()?;
            env.call_method(activity, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&chooser)])?;
            Ok(())
        })
        .map_err(|e| e.to_string())
    }

    fn download(&mut self, release: Release, ctx: &egui::Context) {
        *self.status.lock().unwrap() = Status::Downloading;
        let status = Arc::clone(&self.status);
        let app = self.app.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = with_activity(&app, |env, activity| {
                if env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?.i()? < 29 {
                    return Ok(Err("In-app installation requires Android 10 or newer."));
                }
                let bytes = get_bytes(env, &release.url, MAX_APK_BYTES)?;
                if bytes.is_empty() {
                    return Ok(Err("The APK download was empty or too large."));
                }
                let payload = env.byte_array_from_slice(&bytes)?;
                let algorithm = env.new_string("SHA-256")?;
                let digest = env.call_static_method("java/security/MessageDigest", "getInstance",
                    "(Ljava/lang/String;)Ljava/security/MessageDigest;", &[JValue::Object(&algorithm)])?.l()?;
                let hash = JByteArray::from(env.call_method(digest, "digest", "([B)[B", &[JValue::Object(&payload)])?.l()?);
                let hex: String = env.convert_byte_array(hash)?.iter().map(|b| format!("{b:02x}")).collect();
                if !hex.eq_ignore_ascii_case(&release.sha256) {
                    return Ok(Err("APK checksum did not match. Tap to retry."));
                }
                Ok(Ok(save_apk(env, activity, &release.version_name, &payload)?))
            });
            *status.lock().unwrap() = match result {
                Ok(Ok(uri)) => Status::Downloaded(uri),
                Ok(Err(message)) => Status::Failed(message),
                Err(_) => Status::Failed("Could not download or save the APK. Tap to retry."),
            };
            ctx.request_repaint();
        });
    }

    fn install(&self, uri: String) {
        let message = match open_installer(&self.app, &uri) {
            Ok(true) => None,
            Ok(false) => Some("Allow installs from Winteria in Android Settings, then tap Install update.".into()),
            Err(_) => Some("Android could not open the installer. The APK is in Downloads.".into()),
        };
        *self.status.lock().unwrap() = Status::Ready(uri, message);
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui) {
        let status = self.status.lock().unwrap().clone();
        let elapsed = self.last_check.map_or(CHECK_INTERVAL, |last| last.elapsed());
        if elapsed >= CHECK_INTERVAL && matches!(status, Status::Idle | Status::Current | Status::Failed(_)) {
            self.check(ui.ctx());
            ui.ctx().request_repaint_after(CHECK_INTERVAL);
            return;
        }
        if elapsed < CHECK_INTERVAL {
            ui.ctx().request_repaint_after(CHECK_INTERVAL - elapsed);
        }
        match status {
            Status::Available(release) => {
                if ui.button(format!("Download {}", release.version_name)).clicked() {
                    self.download(release, ui.ctx());
                }
            }
            Status::Downloading => {
                ui.spinner();
                ui.label("Downloading and verifying the APK…");
            }
            Status::Downloaded(uri) => self.install(uri),
            Status::Ready(uri, message) => {
                if let Some(message) = message {
                    ui.label(message);
                }
                if ui.button("Install update").clicked() {
                    self.install(uri);
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

fn get_bytes(env: &mut JNIEnv<'_>, url: &str, limit: usize) -> jni::errors::Result<Vec<u8>> {
    let address = env.new_string(url)?;
    let url = env.new_object("java/net/URL", "(Ljava/lang/String;)V", &[JValue::Object(&address)])?;
    let connection = env.call_method(url, "openConnection", "()Ljava/net/URLConnection;", &[])?.l()?;
    for method in ["setConnectTimeout", "setReadTimeout"] {
        env.call_method(&connection, method, "(I)V", &[JValue::Int(10000)])?;
    }
    env.call_method(&connection, "setUseCaches", "(Z)V", &[JValue::Bool(0)])?;
    let input = env.call_method(&connection, "getInputStream", "()Ljava/io/InputStream;", &[])?.l()?;
    let result = read_stream(env, &input, limit);
    if env.exception_check()? {
        env.exception_clear()?;
    }
    let _ = env.call_method(input, "close", "()V", &[]);
    result
}

fn read_stream(env: &mut JNIEnv<'_>, input: &JObject<'_>, limit: usize) -> jni::errors::Result<Vec<u8>> {
    let buffer = env.new_byte_array(32 * 1024)?;
    let mut bytes = Vec::new();
    loop {
        let count = env.call_method(input, "read", "([B)I", &[JValue::Object(&buffer)])?.i()?;
        if count <= 0 {
            break;
        }
        if bytes.len() + count as usize > limit {
            bytes.clear();
            break;
        }
        bytes.extend_from_slice(&env.convert_byte_array(&buffer)?[..count as usize]);
    }
    Ok(bytes)
}

fn save_apk(
    env: &mut JNIEnv<'_>,
    activity: &JObject<'_>,
    version: &str,
    apk: &JByteArray<'_>,
) -> jni::errors::Result<String> {
    let resolver = env.call_method(activity, "getContentResolver", "()Landroid/content/ContentResolver;", &[])?.l()?;
    let downloads = env.get_static_field("android/provider/MediaStore$Downloads", "EXTERNAL_CONTENT_URI", "Landroid/net/Uri;")?.l()?;
    let values = env.new_object("android/content/ContentValues", "()V", &[])?;
    for (key, value) in [("_display_name", format!("Winteria-{version}.apk")), ("mime_type", APK_MIME.into())] {
        let key = env.new_string(key)?;
        let value = env.new_string(value)?;
        env.call_method(&values, "put", "(Ljava/lang/String;Ljava/lang/String;)V", &[JValue::Object(&key), JValue::Object(&value)])?;
    }
    let pending_key = env.new_string("is_pending")?;
    let pending = |env: &mut JNIEnv<'_>, value| {
        let boxed = env.new_object("java/lang/Integer", "(I)V", &[JValue::Int(value)])?;
        env.call_method(&values, "put", "(Ljava/lang/String;Ljava/lang/Integer;)V", &[JValue::Object(&pending_key), JValue::Object(&boxed)])?;
        Ok::<(), jni::errors::Error>(())
    };
    pending(env, 1)?;
    let uri = env.call_method(&resolver, "insert", "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;",
        &[JValue::Object(&downloads), JValue::Object(&values)])?.l()?;
    let result = (|| {
        let output = env.call_method(&resolver, "openOutputStream", "(Landroid/net/Uri;)Ljava/io/OutputStream;", &[JValue::Object(&uri)])?.l()?;
        let write = env.call_method(&output, "write", "([B)V", &[JValue::Object(apk)]);
        if env.exception_check()? {
            env.exception_clear()?;
        }
        let close = env.call_method(&output, "close", "()V", &[]);
        write?;
        close?;
        env.call_method(&values, "clear", "()V", &[])?;
        pending(env, 0)?;
        env.call_method(&resolver, "update", "(Landroid/net/Uri;Landroid/content/ContentValues;Ljava/lang/String;[Ljava/lang/String;)I",
            &[JValue::Object(&uri), JValue::Object(&values), JValue::Object(&JObject::null()), JValue::Object(&JObject::null())])?;
        let uri = jni::objects::JString::from(env.call_method(&uri, "toString", "()Ljava/lang/String;", &[])?.l()?);
        let address: String = env.get_string(&uri)?.into();
        Ok(address)
    })();
    if result.is_err() {
        if env.exception_check()? {
            env.exception_clear()?;
        }
        let _ = env.call_method(resolver, "delete", "(Landroid/net/Uri;Ljava/lang/String;[Ljava/lang/String;)I",
            &[JValue::Object(&uri), JValue::Object(&JObject::null()), JValue::Object(&JObject::null())]);
    }
    result
}

fn open_installer(app: &AndroidApp, uri: &str) -> jni::errors::Result<bool> {
    with_activity(app, |env, activity| {
        let manager = env.call_method(activity, "getPackageManager", "()Landroid/content/pm/PackageManager;", &[])?.l()?;
        let allowed = env.call_method(manager, "canRequestPackageInstalls", "()Z", &[])?.z()?;
        let package = if allowed { None } else {
            let name = jni::objects::JString::from(env.call_method(activity, "getPackageName", "()Ljava/lang/String;", &[])?.l()?);
            let name: String = env.get_string(&name)?.into();
            Some(format!("package:{name}"))
        };
        let target = env.new_string(package.as_deref().unwrap_or(uri))?;
        let target = env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&target)])?.l()?;
        let action = env.new_string(if allowed { "android.intent.action.VIEW" } else { "android.settings.MANAGE_UNKNOWN_APP_SOURCES" })?;
        let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;Landroid/net/Uri;)V",
            &[JValue::Object(&action), JValue::Object(&target)])?;
        if allowed {
            let mime = env.new_string(APK_MIME)?;
            env.call_method(&intent, "setDataAndType", "(Landroid/net/Uri;Ljava/lang/String;)Landroid/content/Intent;",
                &[JValue::Object(&target), JValue::Object(&mime)])?;
            env.call_method(&intent, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(1)])?;
        }
        env.call_method(activity, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
        Ok(allowed)
    })
}
