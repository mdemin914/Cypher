// FILE: src\audio_engine\looper_track.rs
// ======================================

use crate::looper::SharedLooperState;
use std::collections::BTreeSet;

pub struct Looper {
    pub shared_state: SharedLooperState,
    pub audio: Vec<f32>,
    pub pending_command: bool,
    pub stop_is_queued: bool,
    pub play_is_queued: bool,
    pub cycles_recorded: u32,
    pub playhead: usize,
    pub high_res_summary: Vec<f32>,
    pub samples_since_high_res_update: usize,
    pub peak_since_high_res_update: f32,
    pub samples_since_visual_update: usize,
    pub dirty_summary_chunks: BTreeSet<usize>,
}

impl Looper {
    pub fn new(shared_state: SharedLooperState) -> Self {
        Self {
            shared_state,
            audio: Vec::new(),
            pending_command: false,
            stop_is_queued: false,
            play_is_queued: false,
            cycles_recorded: 0,
            playhead: 0,
            high_res_summary: Vec::new(),
            samples_since_high_res_update: 0,
            peak_since_high_res_update: 0.0,
            samples_since_visual_update: 0,
            dirty_summary_chunks: BTreeSet::new(),
        }
    }
}