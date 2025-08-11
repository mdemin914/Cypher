// src/looper.rs
use std::sync::atomic::{AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

pub const NUM_LOOPERS: usize = 12;
pub const WAVEFORM_DOWNSAMPLE_SIZE: usize = 512;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LooperState {
    Empty,
    Armed,
    Recording,
    Playing,
    Overdubbing,
    Stopped,
}

impl From<u8> for LooperState {
    fn from(val: u8) -> Self {
        match val {
            0 => LooperState::Empty,
            1 => LooperState::Armed,
            2 => LooperState::Recording,
            3 => LooperState::Playing,
            4 => LooperState::Overdubbing,
            5 => LooperState::Stopped,
            _ => LooperState::Empty, // Default fallback
        }
    }
}

/// State that is shared between the UI and audio threads.
#[derive(Clone)]
pub struct SharedLooperState {
    state: Arc<AtomicU8>,
    length_in_cycles: Arc<AtomicU32>,
    playhead: Arc<AtomicUsize>,
    waveform_summary: Arc<RwLock<Vec<f32>>>,
    stop_is_queued: Arc<AtomicU8>,
    pending_command: Arc<AtomicU8>,
    is_playing: Arc<AtomicU8>,
}

impl SharedLooperState {
    pub fn new() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(LooperState::Empty as u8)),
            length_in_cycles: Arc::new(AtomicU32::new(0)),
            playhead: Arc::new(AtomicUsize::new(0)),
            waveform_summary: Arc::new(RwLock::new(Vec::new())),
            stop_is_queued: Arc::new(AtomicU8::new(0)),
            pending_command: Arc::new(AtomicU8::new(0)),
            is_playing: Arc::new(AtomicU8::new(0)),
        }
    }

    pub fn get(&self) -> LooperState {
        self.state.load(Ordering::Relaxed).into()
    }

    pub fn set(&self, state: LooperState) {
        self.state.store(state as u8, Ordering::Relaxed);
    }

    pub fn get_length_in_cycles(&self) -> u32 {
        self.length_in_cycles.load(Ordering::Relaxed)
    }

    pub fn set_length_in_cycles(&self, cycles: u32) {
        self.length_in_cycles
            .store(cycles, Ordering::Relaxed);
    }

    pub fn get_playhead(&self) -> usize {
        self.playhead.load(Ordering::Relaxed)
    }

    pub fn set_playhead(&self, playhead: usize) {
        self.playhead.store(playhead, Ordering::Relaxed);
    }

    pub fn get_waveform_summary(&self) -> Arc<RwLock<Vec<f32>>> {
        self.waveform_summary.clone()
    }

    pub fn get_stop_is_queued(&self) -> bool {
        self.stop_is_queued.load(Ordering::Relaxed) != 0
    }

    pub fn set_stop_is_queued(&self, queued: bool) {
        self.stop_is_queued.store(if queued { 1 } else { 0 }, Ordering::Relaxed);
    }

    pub fn get_pending_command(&self) -> bool {
        self.pending_command.load(Ordering::Relaxed) != 0
    }

    pub fn set_pending_command(&self, pending: bool) {
        self.pending_command.store(if pending { 1 } else { 0 }, Ordering::Relaxed);
    }

    pub fn get_is_playing(&self) -> bool {
        self.is_playing.load(Ordering::Relaxed) != 0
    }

    pub fn set_is_playing(&self, playing: bool) {
        self.is_playing.store(if playing { 1 } else { 0 }, Ordering::Relaxed);
    }
}