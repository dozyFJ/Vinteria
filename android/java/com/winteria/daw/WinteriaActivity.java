package com.winteria.daw;

import android.app.NativeActivity;
import android.content.ClipData;
import android.content.Intent;
import android.os.Bundle;
import android.database.Cursor;
import android.net.Uri;
import android.media.AudioAttributes;
import android.media.AudioManager;
import android.media.MediaPlayer;
import android.provider.DocumentsContract;
import android.provider.OpenableColumns;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.PrintWriter;
import java.io.StringWriter;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;
import java.util.Locale;
import java.util.zip.ZipEntry;
import java.util.zip.ZipInputStream;

public final class WinteriaActivity extends NativeActivity {
    private static final int PICK_SOUNDS = 4501;
    private static final int PICK_PACK_ZIP = 4502;
    private static final int PICK_PACK_DIR = 4503;
    private static final int PICK_PROJECT = 4504;
    private static final long MAX_PROJECT_SIZE = 128L * 1024 * 1024;
    private static final long MAX_WAV_SIZE = 64L * 1024 * 1024;
    private static final long MAX_PACK_SIZE = 2L * 1024 * 1024 * 1024;
    private static final int MAX_PACK_FILES = 5000;
    private volatile String importStatus;
    private volatile String projectImportResult;
    private File projectImportDirectory;
    private boolean importPending;

    @Override
    protected void onCreate(Bundle state) {
        // a Java error would close the app without a trace: write it where the Rust side shows crash reports (crash.txt)
        final Thread.UncaughtExceptionHandler previous = Thread.getDefaultUncaughtExceptionHandler();
        final File report = new File(getFilesDir(), "crash.txt");
        Thread.setDefaultUncaughtExceptionHandler((thread, failure) -> {
            try (FileOutputStream out = new FileOutputStream(report)) {
                StringWriter trace = new StringWriter();
                failure.printStackTrace(new PrintWriter(trace));
                out.write(("Winteria crashed (Java)\n[thread " + thread.getName() + "]\n\n" + trace).getBytes("UTF-8"));
            } catch (Throwable ignored) {
            }
            if (previous != null) {
                previous.uncaughtException(thread, failure);
            }
        });
        super.onCreate(state);
    }

    public void playStartupSound() {
        runOnUiThread(() -> {
            if (isFinishing() || isDestroyed()) {
                return;
            }
            MediaPlayer player = null;
            try {
                AudioManager manager = getSystemService(AudioManager.class);
                if (manager == null || manager.getRingerMode() != AudioManager.RINGER_MODE_NORMAL) {
                    return;
                }
                int sound = getResources().getIdentifier("winteria_startup", "raw", getPackageName());
                if (sound == 0) {
                    return;
                }
                AudioAttributes attributes = new AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                    .build();
                player = MediaPlayer.create(this, sound, attributes, AudioManager.AUDIO_SESSION_ID_GENERATE);
                if (player != null) {
                    player.setOnCompletionListener(MediaPlayer::release);
                    player.setOnErrorListener((failedPlayer, what, extra) -> {
                        failedPlayer.release();
                        return true;
                    });
                    player.start();
                }
            } catch (RuntimeException ignored) {
                if (player != null) {
                    player.release();
                }
            }
        });
    }

    private static final int REQUEST_MICROPHONE = 430;
    private volatile boolean microphoneRequestPending;

    public void requestMicrophonePermission() {
        if (microphoneRequestPending || microphonePermissionState() == 1) {
            return;
        }
        microphoneRequestPending = true;
        runOnUiThread(() -> requestPermissions(
            new String[] {android.Manifest.permission.RECORD_AUDIO}, REQUEST_MICROPHONE));
    }

    public int microphonePermissionState() {
        if (checkSelfPermission(android.Manifest.permission.RECORD_AUDIO)
                == android.content.pm.PackageManager.PERMISSION_GRANTED) {
            return 1;
        }
        return microphoneRequestPending ? 0 : -1;
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        if (requestCode == REQUEST_MICROPHONE) {
            microphoneRequestPending = false;
        }
    }

    public void chooseProject(String directory) {
        runOnUiThread(() -> {
            projectImportResult = null;
            if (importPending) {
                projectImportResult = "Another file import is already open.";
                return;
            }
            Intent picker = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            picker.setType("*/*");
            picker.addCategory(Intent.CATEGORY_OPENABLE);
            try {
                projectImportDirectory = new File(directory);
                startActivityForResult(picker, PICK_PROJECT);
                importPending = true;
            } catch (RuntimeException failure) {
                projectImportDirectory = null;
                projectImportResult = "Could not open the Android file picker.";
            }
        });
    }

    public synchronized String takeProjectImportResult() {
        String result = projectImportResult;
        projectImportResult = null;
        return result;
    }

    private void onProjectPicked(int result, Intent data) {
        File directory = projectImportDirectory;
        projectImportDirectory = null;
        Uri uri = data == null ? null : data.getData();
        if (result != RESULT_OK || uri == null) {
            importPending = false;
            projectImportResult = "Project import canceled.";
            return;
        }
        if (directory == null) {
            importPending = false;
            projectImportResult = "The project folder is unavailable. Choose the file again.";
            return;
        }
        new Thread(() -> {
            String outcome;
            try {
                outcome = stageProject(uri, directory);
            } catch (IOException | RuntimeException failure) {
                outcome = "Import failed: " + String.valueOf(failure.getMessage()).replace('\n', ' ');
            }
            final String completed = outcome;
            runOnUiThread(() -> {
                importPending = false;
                synchronized (this) {
                    projectImportResult = completed;
                }
            });
        }, "winteria-project-import").start();
    }

    private String stageProject(Uri uri, File directory) throws IOException {
        String display = displayName(uri);
        int dot = display.lastIndexOf('.');
        String ext = dot < 0 ? "" : display.substring(dot + 1).toLowerCase(Locale.ROOT);
        if (!ext.equals("wd") && !ext.equals("vpd") && !ext.equals("synthesia")) {
            throw new IOException("Choose a .wd, .vpd or .synthesia project.");
        }
        String name = safeName(display.substring(0, dot), "Project") + "." + ext;
        File staged = File.createTempFile(".project-import-", ".tmp", directory);
        boolean copied = false;
        try {
            try (InputStream source = getContentResolver().openInputStream(uri);
                 FileOutputStream output = new FileOutputStream(staged)) {
                if (source == null) {
                    throw new IOException("Could not read the project document.");
                }
                byte[] buffer = new byte[32768];
                long total = 0;
                int length;
                while ((length = source.read(buffer)) != -1) {
                    total += length;
                    if (total > MAX_PROJECT_SIZE) {
                        throw new IOException("The project exceeds the 128 MB import limit.");
                    }
                    output.write(buffer, 0, length);
                }
            }
            copied = true;
            return name + "\n" + staged.getAbsolutePath();
        } finally {
            if (!copied) {
                staged.delete();
            }
        }
    }

    public void chooseSounds() {
        runOnUiThread(() -> {
            if (importPending) {
                return;
            }
            importStatus = null;
            Intent picker = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            picker.setType("audio/*");
            picker.addCategory(Intent.CATEGORY_OPENABLE);
            picker.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true);
            try {
                startActivityForResult(picker, PICK_SOUNDS);
                importPending = true;
            } catch (RuntimeException failure) {
                importStatus = "Could not open the Android file picker.";
            }
        });
    }

    /** sample pack: a ZIP (folder = false) or a whole folder (folder = true), WAVs land in samples/<pack name> */
    public void choosePack(boolean folder) {
        runOnUiThread(() -> {
            if (importPending) {
                return;
            }
            importStatus = null;
            Intent picker;
            if (folder) {
                picker = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
            } else {
                picker = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                picker.setType("*/*");
                picker.addCategory(Intent.CATEGORY_OPENABLE);
                picker.putExtra(Intent.EXTRA_MIME_TYPES, new String[]{
                    "application/zip", "application/x-zip-compressed", "application/x-zip", "application/octet-stream"});
            }
            try {
                startActivityForResult(picker, folder ? PICK_PACK_DIR : PICK_PACK_ZIP);
                importPending = true;
            } catch (RuntimeException failure) {
                importStatus = "Could not open the Android file picker.";
            }
        });
    }

    public String takeSoundImportStatus() {
        String status = importStatus;
        if (status != null) {
            importStatus = null;
        }
        return status;
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request == PICK_PROJECT) {
            onProjectPicked(result, data);
            return;
        }
        if (request == PICK_PACK_ZIP || request == PICK_PACK_DIR) {
            onPackPicked(request == PICK_PACK_DIR, result, data);
            return;
        }
        if (request != PICK_SOUNDS) {
            return;
        }
        if (result != RESULT_OK || data == null) {
            importPending = false;
            importStatus = "Sound import canceled.";
            return;
        }
        new Thread(() -> {
            int imported = 0;
            int rejected = 0;
            ClipData selection = data.getClipData();
            int count = selection == null ? 1 : selection.getItemCount();
            for (int index = 0; index < count; index++) {
                Uri uri = selection == null ? data.getData() : selection.getItemAt(index).getUri();
                try {
                    importWav(uri);
                    imported++;
                } catch (Throwable failure) {
                    rejected++;
                }
            }
            final String resultText = "Imported " + imported + " WAV file(s)"
                + (rejected == 0 ? "." : "; skipped " + rejected + " unreadable, non-WAV or oversized file(s).");
            runOnUiThread(() -> {
                importPending = false;
                importStatus = resultText;
            });
        }, "winteria-sample-import").start();
    }

    private void importWav(Uri uri) throws IOException {
        if (uri == null) {
            throw new IOException("No document URI");
        }
        File directory = new File(getFilesDir(), "samples/Imported");
        if (!directory.isDirectory() && !directory.mkdirs()) {
            throw new IOException("Could not create the library folder");
        }
        String name = displayName(uri).replaceAll("[\\x00-\\x1f/\\\\]", "_").trim();
        if (name.toLowerCase(java.util.Locale.ROOT).endsWith(".wav")) {
            name = name.substring(0, name.length() - 4);
        }
        if (name.length() > 100) {
            name = name.substring(0, 100);
        }
        if (name.isEmpty()) {
            name = "Sound";
        }

        File temporary = File.createTempFile("import-", ".tmp", directory);
        try {
            try (InputStream source = getContentResolver().openInputStream(uri);
                 FileOutputStream target = new FileOutputStream(temporary)) {
                if (source == null) {
                    throw new IOException("Could not read the document");
                }
                byte[] buffer = new byte[32768];
                int header = 0;
                while (header < 12) {
                    int length = source.read(buffer, header, 12 - header);
                    if (length <= 0) {
                        throw new IOException("Invalid WAV header");
                    }
                    header += length;
                }
                if (buffer[0] != 'R' || buffer[1] != 'I' || buffer[2] != 'F' || buffer[3] != 'F'
                    || buffer[8] != 'W' || buffer[9] != 'A' || buffer[10] != 'V' || buffer[11] != 'E') {
                    throw new IOException("Document is not a WAV");
                }
                target.write(buffer, 0, header);
                long total = header;
                int length;
                while ((length = source.read(buffer)) != -1) {
                    total += length;
                    if (total > MAX_WAV_SIZE) {
                        throw new IOException("WAV exceeds the import size limit");
                    }
                    target.write(buffer, 0, length);
                }
            }
            File destination = new File(directory, name + ".wav");
            for (int suffix = 2; destination.exists(); suffix++) {
                destination = new File(directory, name + " " + suffix + ".wav");
            }
            Files.move(temporary.toPath(), destination.toPath(), StandardCopyOption.ATOMIC_MOVE);
        } finally {
            temporary.delete();
        }
    }

    private static final class PackCounter {
        int files;
        int skipped;
        long bytes;
    }

    private void onPackPicked(boolean folder, int result, Intent data) {
        Uri uri = data == null ? null : data.getData();
        if (result != RESULT_OK || uri == null) {
            importPending = false;
            importStatus = "Pack import canceled.";
            return;
        }
        new Thread(() -> {
            String text;
            File staging = null;
            try {
                File samples = new File(getFilesDir(), "samples");
                if (!samples.isDirectory() && !samples.mkdirs()) {
                    throw new IOException("Could not create the library folder");
                }
                String name = folder ? treeName(uri) : displayName(uri);
                if (name.toLowerCase(Locale.ROOT).endsWith(".zip")) {
                    name = name.substring(0, name.length() - 4);
                }
                name = safeName(name, "Pack");
                staging = new File(samples, ".import-" + System.nanoTime());
                if (!staging.mkdirs()) {
                    throw new IOException("Could not create a temporary folder");
                }
                PackCounter count = new PackCounter();
                if (folder) {
                    copyTree(uri, DocumentsContract.getTreeDocumentId(uri), staging, 0, count);
                } else {
                    unzip(uri, staging, count);
                }
                if (count.files == 0) {
                    text = "No WAV files found in the pack.";
                } else {
                    File destination = new File(samples, name);
                    for (int suffix = 2; destination.exists(); suffix++) {
                        destination = new File(samples, name + " " + suffix);
                    }
                    if (!staging.renameTo(destination)) {
                        throw new IOException("Could not move the pack into the library");
                    }
                    staging = null;
                    text = "Pack \"" + destination.getName() + "\": " + count.files + " WAV file(s)"
                        + (count.skipped == 0 ? "." : ", skipped " + count.skipped + " other or oversized file(s).");
                }
            } catch (Throwable failure) {
                // never let an import error take the app down
                text = "Pack import failed: " + failure;
            } finally {
                if (staging != null) {
                    deleteRecursively(staging);
                }
            }
            final String resultText = text;
            runOnUiThread(() -> {
                importPending = false;
                importStatus = resultText;
            });
        }, "winteria-pack-import").start();
    }

    private void unzip(Uri uri, File root, PackCounter count) throws IOException {
        try (InputStream raw = getContentResolver().openInputStream(uri)) {
            if (raw == null) {
                throw new IOException("Could not read the ZIP");
            }
            try (ZipInputStream zip = new ZipInputStream(raw)) {
                ZipEntry entry;
                boolean any = false;
                while ((entry = zip.getNextEntry()) != null) {
                    any = true;
                    if (entry.isDirectory()) {
                        continue;
                    }
                    String relative = safeRelative(entry.getName());
                    if (relative == null || !relative.toLowerCase(Locale.ROOT).endsWith(".wav")) {
                        count.skipped++;
                        continue;
                    }
                    writeWav(zip, new File(root, relative), count);
                }
                if (!any) {
                    throw new IOException("not a ZIP file");
                }
            }
        }
    }

    private void copyTree(Uri tree, String documentId, File target, int depth, PackCounter count) throws IOException {
        if (depth > 8) {
            return;
        }
        Uri children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, documentId);
        String[] columns = {DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE};
        try (Cursor cursor = getContentResolver().query(children, columns, null, null, null)) {
            while (cursor != null && cursor.moveToNext()) {
                String id = cursor.getString(0);
                String name = cursor.isNull(1) ? "" : cursor.getString(1);
                String mime = cursor.isNull(2) ? "" : cursor.getString(2);
                if (name.startsWith(".")) {
                    continue;
                }
                if (DocumentsContract.Document.MIME_TYPE_DIR.equals(mime)) {
                    copyTree(tree, id, new File(target, safeName(name, "Folder")), depth + 1, count);
                } else if (name.toLowerCase(Locale.ROOT).endsWith(".wav")) {
                    try (InputStream source = getContentResolver().openInputStream(DocumentsContract.buildDocumentUriUsingTree(tree, id))) {
                        if (source == null) {
                            count.skipped++;
                            continue;
                        }
                        writeWav(source, new File(target, safeName(name, "Sound.wav")), count);
                    }
                } else {
                    count.skipped++;
                }
            }
        }
    }

    /** copies one WAV (checks the RIFF/WAVE header and the size limits); a bad file is skipped, not fatal */
    private void writeWav(InputStream source, File destination, PackCounter count) throws IOException {
        if (count.files >= MAX_PACK_FILES) {
            count.skipped++;
            return;
        }
        File parent = destination.getParentFile();
        if (parent != null && !parent.isDirectory() && !parent.mkdirs()) {
            throw new IOException("Could not create a pack folder");
        }
        boolean ok = false;
        long written = 0;
        try (FileOutputStream out = new FileOutputStream(destination)) {
            byte[] buffer = new byte[32768];
            int header = 0;
            while (header < 12) {
                int length = source.read(buffer, header, 12 - header);
                if (length <= 0) {
                    break;
                }
                header += length;
            }
            if (header == 12 && buffer[0] == 'R' && buffer[1] == 'I' && buffer[2] == 'F' && buffer[3] == 'F'
                && buffer[8] == 'W' && buffer[9] == 'A' && buffer[10] == 'V' && buffer[11] == 'E') {
                out.write(buffer, 0, header);
                written = header;
                ok = true;
                int length;
                while ((length = source.read(buffer)) != -1) {
                    written += length;
                    if (written > MAX_WAV_SIZE || count.bytes + written > MAX_PACK_SIZE) {
                        ok = false;
                        break;
                    }
                    out.write(buffer, 0, length);
                }
            }
        }
        if (ok) {
            count.files++;
            count.bytes += written;
        } else {
            destination.delete();
            count.skipped++;
        }
    }

    /** a ZIP entry path inside the pack folder: no absolute paths, no "..", no hidden or macOS metadata entries */
    private static String safeRelative(String entry) {
        StringBuilder out = new StringBuilder();
        String[] parts = entry.replace('\\', '/').split("/");
        for (int i = 0; i < parts.length; i++) {
            String part = parts[i].trim();
            if (part.isEmpty() || part.equals(".")) {
                continue;
            }
            if (part.equals("..") || part.startsWith(".") || part.equals("__MACOSX")) {
                return null;
            }
            if (out.length() > 0) {
                out.append('/');
            }
            out.append(safeName(part, "_"));
        }
        return out.length() == 0 ? null : out.toString();
    }

    private static String safeName(String name, String fallback) {
        String clean = name.replaceAll("[\\x00-\\x1f/\\\\:]", "_").trim();
        if (clean.length() > 100) {
            clean = clean.substring(0, 100);
        }
        return clean.isEmpty() || clean.startsWith(".") ? fallback : clean;
    }

    private String treeName(Uri tree) {
        String id = DocumentsContract.getTreeDocumentId(tree);
        try (Cursor cursor = getContentResolver().query(DocumentsContract.buildDocumentUriUsingTree(tree, id),
            new String[]{DocumentsContract.Document.COLUMN_DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst() && !cursor.isNull(0)) {
                return cursor.getString(0);
            }
        } catch (RuntimeException ignored) {
        }
        int colon = id.lastIndexOf(':');
        String tail = colon >= 0 ? id.substring(colon + 1) : id;
        int slash = tail.lastIndexOf('/');
        return slash >= 0 ? tail.substring(slash + 1) : tail;
    }

    private static void deleteRecursively(File file) {
        File[] children = file.listFiles();
        if (children != null) {
            for (File child : children) {
                deleteRecursively(child);
            }
        }
        file.delete();
    }

    private String displayName(Uri uri) {
        try (Cursor cursor = getContentResolver().query(uri, new String[]{OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst() && !cursor.isNull(0)) {
                return cursor.getString(0);
            }
        }
        return "Sound.wav";
    }
}
