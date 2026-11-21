package com.winteria.daw;

import android.app.NativeActivity;
import android.content.ClipData;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.provider.OpenableColumns;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;

public final class WinteriaActivity extends NativeActivity {
    private static final int PICK_SOUNDS = 4501;
    private static final long MAX_WAV_SIZE = 64L * 1024 * 1024;
    private volatile String importStatus;
    private boolean importPending;

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
                } catch (IOException | RuntimeException failure) {
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

    private String displayName(Uri uri) {
        try (Cursor cursor = getContentResolver().query(uri, new String[]{OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst() && !cursor.isNull(0)) {
                return cursor.getString(0);
            }
        }
        return "Sound.wav";
    }
}
