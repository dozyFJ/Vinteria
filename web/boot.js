import init, { start_winteria, load_sample, voice_result } from './pkg/winteria.js';

const open = document.querySelector('#open');
const launcher = document.querySelector('#launcher');
const status = document.querySelector('#status');

function showError(error) {
  status.textContent = String(error);
  open.disabled = false;
  open.textContent = 'Повторить';
}

open.addEventListener('click', async () => {
  open.disabled = true;
  try {
    await init();
    await start_winteria(document.querySelector('#studio'));
    launcher.hidden = true;
  } catch (error) {
    showError(error);
  }
});

const picker = document.querySelector('#sound-picker');
const sounds = document.querySelector('#sounds');
const soundStatus = document.querySelector('#sound-status');

window.winteriaChooseSounds = folder => {
  sounds.toggleAttribute('webkitdirectory', folder);
  document.querySelector('#sound-label').textContent = folder ? 'Выбрать папку' : 'Выбрать WAV';
  soundStatus.textContent = '';
  sounds.value = '';
  picker.showModal();
};

function sampleDb() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open('winteria-sounds', 1);
    request.onupgradeneeded = () => request.result.createObjectStore('wav');
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

function dbOperation(db, mode, operation) {
  return new Promise((resolve, reject) => {
    const tx = db.transaction('wav', mode);
    const request = operation(tx.objectStore('wav'));
    tx.oncomplete = () => resolve(request.result);
    tx.onabort = () => reject(tx.error);
    tx.onerror = () => reject(tx.error);
  });
}

sounds.addEventListener('change', async () => {
  let db;
  try {
    db = await sampleDb();
    const wavs = Array.from(sounds.files).filter(file => /\.wav$/i.test(file.name));
    let count = 0;
    for (const file of wavs) {
      const path = file.webkitRelativePath || file.name;
      const bytes = new Uint8Array(await file.arrayBuffer());
      try {
        load_sample(path, bytes, true);
        await dbOperation(db, 'readwrite', store => store.put(bytes, path));
        count++;
      } catch (error) {
        console.warn('Could not import', path, error);
      }
    }
    soundStatus.textContent = `Сохранено WAV: ${count} из ${wavs.length}`;
  } catch (error) {
    soundStatus.textContent = String(error);
  } finally {
    db?.close();
  }
});

async function restoreSounds() {
  const db = await sampleDb();
  try {
    const records = await dbOperation(db, 'readonly', store => store.getAll());
    const paths = await dbOperation(db, 'readonly', store => store.getAllKeys());
    records.forEach((bytes, index) => load_sample(paths[index], bytes, false));
  } finally {
    db.close();
  }
}

init().then(() => restoreSounds().catch(console.warn)).then(() => {
  open.disabled = false;
  open.textContent = 'Открыть Studio';
}).catch(showError);

if ('serviceWorker' in navigator) {
  navigator.serviceWorker.register('./sw.js').catch(console.warn);
}

window.winteriaDownloadWav = (name, bytes) => {
  const link = document.createElement('a');
  link.download = name;
  link.href = URL.createObjectURL(new Blob([bytes.slice()], { type: 'audio/wav' }));
  link.click();
  setTimeout(() => URL.revokeObjectURL(link.href), 1000);
};

let voiceCapture = null;

window.winteriaRecordVoice = async start => {
  if (!start) {
    if (voiceCapture) {
      voiceCapture.cancelled = true;
      if (voiceCapture.recorder?.state === 'recording') voiceCapture.recorder.stop();
    }
    return;
  }
  if (voiceCapture) return;
  const capture = { cancelled: false };
  voiceCapture = capture;
  let stream;
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    if (capture.cancelled) {
      stream.getTracks().forEach(track => track.stop());
      voiceCapture = null;
      return;
    }
    const recorder = new MediaRecorder(stream);
    capture.recorder = recorder;
    const chunks = [];
    recorder.ondataavailable = event => chunks.push(event.data);
    recorder.onstop = async () => {
      stream.getTracks().forEach(track => track.stop());
      let audio, db;
      try {
        audio = new AudioContext();
        const blob = new Blob(chunks, { type: recorder.mimeType });
        const recording = await audio.decodeAudioData(await blob.arrayBuffer());
        const bytes = encodeVoice(recording);
        const name = `Voice ${new Date().toISOString().replace(/[:.]/g, '-')}.wav`;
        load_sample(name, bytes, true);
        db = await sampleDb();
        await dbOperation(db, 'readwrite', store => store.put(bytes, name));
        voice_result(name);
      } catch (error) {
        voice_result('', String(error));
      } finally {
        db?.close();
        await audio?.close();
        voiceCapture = null;
      }
    };
    recorder.start();
  } catch (error) {
    stream?.getTracks().forEach(track => track.stop());
    voice_result('', String(error));
    voiceCapture = null;
  }
};

function encodeVoice(audio) {
  const length = Math.min(audio.length, audio.sampleRate * 60);
  const bytes = new Uint8Array(44 + length * 2);
  const header = new DataView(bytes.buffer);
  const text = (offset, value) => bytes.set(new TextEncoder().encode(value), offset);
  text(0, 'RIFF');
  header.setUint32(4, bytes.length - 8, true);
  text(8, 'WAVEfmt ');
  header.setUint32(16, 16, true);
  header.setUint16(20, 1, true);
  header.setUint16(22, 1, true);
  header.setUint32(24, audio.sampleRate, true);
  header.setUint32(28, audio.sampleRate * 2, true);
  header.setUint16(32, 2, true);
  header.setUint16(34, 16, true);
  text(36, 'data');
  header.setUint32(40, length * 2, true);
  const samples = audio.getChannelData(0);
  for (let i = 0; i < length; i++) {
    header.setInt16(44 + i * 2, Math.round(Math.max(-1, Math.min(1, samples[i])) * 32767), true);
  }
  return bytes;
}
