//! MIDI keyboard input: notes go straight to the audio engine, and are forwarded to the UI for recording.
use crate::audio_engine::device::Shared;
use crate::audio_engine::Cmd;
use crate::piano_roll::Track;
use crossbeam_channel::{bounded, Receiver, Sender};
#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
use midir::{MidiInput, MidiInputConnection};
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct MidiEvent {
    pub on: bool,
    pub pitch: u8,
    pub vel: u8,
}

#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
pub struct MidiIn {
    pub ports: Vec<String>,
    pub selected: Option<String>,
    conn: Option<MidiInputConnection<()>>,
    pub rx: Receiver<MidiEvent>,
    tx: Sender<MidiEvent>,
    shared: Arc<Shared>,
    pub error: Option<String>,
}

#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
impl MidiIn {
    pub fn new(shared: Arc<Shared>) -> Self {
        let (tx, rx) = bounded(256);
        let mut m = MidiIn { ports: vec![], selected: None, conn: None, rx, tx, shared, error: None };
        m.refresh();
        m
    }

    pub fn refresh(&mut self) {
        self.ports = match MidiInput::new("synthesia-list") {
            Ok(mi) => mi.ports().iter().filter_map(|p| mi.port_name(p).ok()).collect(),
            Err(_) => vec![],
        };
    }

    pub fn disconnect(&mut self) {
        self.conn = None;
        self.selected = None;
    }

    pub fn connect(&mut self, name: &str) {
        self.conn = None;
        self.error = None;
        let mi = match MidiInput::new("synthesia") {
            Ok(m) => m,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        let Some(port) = mi.ports().into_iter().find(|p| mi.port_name(p).map(|n| n == name).unwrap_or(false)) else {
            self.error = Some(format!("MIDI port '{name}' not found"));
            return;
        };
        let tx = self.tx.clone();
        let shared = Arc::clone(&self.shared);
        let res = mi.connect(
            &port,
            "synthesia-in",
            move |_ts, msg, _| {
                if msg.len() < 3 {
                    return;
                }
                let kind = msg[0] & 0xF0;
                let (pitch, vel) = (msg[1] & 0x7F, msg[2] & 0x7F);
                let on = kind == 0x90 && vel > 0;
                let off = kind == 0x80 || (kind == 0x90 && vel == 0);
                if !on && !off {
                    return;
                }
                let track = Track::from_id(shared.live_track.load(Ordering::Relaxed));
                shared.send(if on { Cmd::NoteOn { track, pitch, vel } } else { Cmd::NoteOff { track, pitch } });
                let _ = tx.try_send(MidiEvent { on, pitch, vel });
            },
            (),
        );
        match res {
            Ok(c) => {
                self.conn = Some(c);
                self.selected = Some(name.to_string());
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
}

/// Android: no MIDI input in the first beta.
#[cfg(any(target_os = "android", target_arch = "wasm32"))]
pub struct MidiIn {
    pub ports: Vec<String>,
    pub selected: Option<String>,
    pub rx: Receiver<MidiEvent>,
    pub error: Option<String>,
}

#[cfg(any(target_os = "android", target_arch = "wasm32"))]
impl MidiIn {
    pub fn new(_shared: Arc<Shared>) -> Self {
        let (_tx, rx) = bounded(1);
        MidiIn { ports: vec![], selected: None, rx, error: None }
    }
    pub fn refresh(&mut self) {}
    pub fn disconnect(&mut self) {
        self.selected = None;
    }
    pub fn connect(&mut self, _name: &str) {}
}
