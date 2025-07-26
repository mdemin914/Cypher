// src/app.rs
use crate::asset::{Asset, AssetLibrary, SamplerKitRef, SampleRef, SessionRef, SynthPresetRef};
use crate::audio_device;
use crate::audio_engine::{AudioCommand, AudioEngine, MidiMessage};
use crate::audio_io;
use crate::looper::{SharedLooperState, NUM_LOOPERS};
use crate::midi;
use crate::mixer::{MixerState, MixerTrackState};
use crate::preset::{SynthEnginePreset, SynthPreset};
use crate::sampler::SamplerKit;
use crate::sampler_engine::{self, NUM_SAMPLE_SLOTS};
use crate::settings::{self, AppSettings, ControllableParameter, MidiControlId};
use crate::slicer;
use crate::synth::{
    EngineParamsUnion, EngineWithVolumeAndPeak, LfoRateMode, ModSource, SamplerParams,
    WavetableParams, WAVETABLE_SIZE,
};
use crate::theme::Theme;
use crate::theory::{self, ChordStyle, Scale};
use crate::ui;
use crate::wavetable_engine::{self, WavetableEnginePreset, WavetableSet, WavetableSource};
use anyhow::{anyhow, Result};
use chrono::Local;
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, HostId, Stream};
use egui::Color32;
use midir::{MidiInputConnection, MidiInputPort};
use rfd::FileDialog;
use ringbuf::HeapRb;
use rodio::source::Source;
use rodio::Decoder;
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Instant;
use walkdir::WalkDir;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum LibraryView {
    Samples,
    Synths,
    Kits,
    Sessions,
    EightyEightKeys,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SessionData {
    pub mixer_state: MixerState,
    pub synth_preset_path: Option<PathBuf>,
    pub sampler_kit_path: Option<PathBuf>,
    pub is_input_armed: bool,
    pub is_input_monitored: bool,
    pub transport_len_samples: usize,
    pub original_sample_rate: u32,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum TheoryMode {
    Scales,
    Chords,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum ChordDisplayMode {
    Spread,
    Stacked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SynthUISection {
    // Wavetable specific
    Wavetable,
    // Sampler specific
    Sampler,
    // Shared
    Saturation,
    Filter,
    VolumeEnv,
    FilterEnv,
    Lfo1,
    Lfo2,
    ModMatrix,
}

impl std::fmt::Display for SynthUISection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SynthUISection::Wavetable => write!(f, "Wavetable"),
            SynthUISection::Sampler => write!(f, "Sampler"),
            SynthUISection::Saturation => write!(f, "Saturation"),
            SynthUISection::Filter => write!(f, "Filter"),
            SynthUISection::VolumeEnv => write!(f, "Env 1"),
            SynthUISection::FilterEnv => write!(f, "Env 2"),
            SynthUISection::Lfo1 => write!(f, "LFO 1"),
            SynthUISection::Lfo2 => write!(f, "LFO 2"),
            SynthUISection::ModMatrix => write!(f, "Mod Matrix"),
        }
    }
}

pub enum EngineState {
    Wavetable(wavetable_engine::WavetableEngineState),
    Sampler(sampler_engine::SamplerEngineState),
}

impl EngineState {
    fn new_wavetable() -> Self {
        EngineState::Wavetable(wavetable_engine::WavetableEngineState::new())
    }
    fn new_sampler() -> Self {
        EngineState::Sampler(sampler_engine::SamplerEngineState::new())
    }
}

pub struct SlicerState {
    pub source_audio: Option<SourceAudio>,
    pub slice_regions: Vec<(usize, usize)>,
    pub threshold: f32,
    pub min_silence_ms: f32,
    pub tail_ms: f32,
    pub base_export_name: String,
    pub export_parent_path: PathBuf,
    pub export_new_folder_name: String,
    pub view_start_sample: usize,
    pub view_end_sample: usize,
}

impl SlicerState {
    pub fn new() -> Self {
        Self {
            source_audio: None,
            slice_regions: Vec::new(),
            threshold: 0.012,
            min_silence_ms: 1000.0,
            tail_ms: 3000.0,
            base_export_name: "slice".to_string(),
            export_parent_path: PathBuf::new(),
            export_new_folder_name: "New Slices".to_string(),
            view_start_sample: 0,
            view_end_sample: 0,
        }
    }
}

pub struct CypherApp {
    // --- App State ---
    pub options_window_open: bool,
    pub sample_pad_window_open: bool,
    pub synth_editor_window_open: bool,
    pub theme_editor_window_open: bool,
    pub slicer_window_open: bool,
    pub midi_mapping_window_open: bool,
    pub is_recording_output: bool,
    pub recording_notification: Option<(String, Instant)>,
    pub library_path: Vec<String>,
    pub settings: AppSettings,
    pub library_view: LibraryView,
    pub asset_library: AssetLibrary,
    pub theme: Theme,
    pub available_themes: Vec<(String, PathBuf)>,
    pub active_synth_section: [SynthUISection; 2],
    pub bpm_rounding_setting_changed_unapplied: bool,
    pub current_session_path: Option<PathBuf>,

    // --- Audio Engine Resources (managed) ---
    _input_stream: Option<Stream>,
    _output_stream: Option<Stream>,
    _midi_connection: Option<MidiInputConnection<()>>,
    _command_thread_handle: Option<JoinHandle<()>>,
    command_sender: Option<mpsc::Sender<AudioCommand>>,

    // --- UI / Shared State ---
    pub looper_states: Vec<SharedLooperState>,
    pub transport_playhead: Arc<AtomicUsize>,
    pub transport_len_samples: Arc<AtomicUsize>,
    pub transport_is_playing: Arc<AtomicBool>,
    pub synth_is_active: Arc<AtomicBool>,
    pub audio_input_is_armed: Arc<AtomicBool>,
    pub audio_input_is_monitored: Arc<AtomicBool>,
    pub sampler_is_active: Arc<AtomicBool>,
    pub sampler_pad_info: [Option<SampleRef>; 16],
    pub playing_pads: Arc<AtomicU16>,
    pub cpu_load: Arc<AtomicU32>,
    pub xrun_count: Arc<AtomicUsize>,
    pub live_midi_notes: Arc<RwLock<BTreeSet<u8>>>,
    pub should_toggle_record_from_midi: Arc<AtomicBool>,
    pub midi_cc_values: Arc<[[AtomicU32; 128]; 16]>,

    // --- Mixer State ---
    pub track_mixer_state: Arc<RwLock<MixerState>>,
    pub peak_meters: Arc<[AtomicU32; NUM_LOOPERS]>,
    pub displayed_peak_levels: [f32; NUM_LOOPERS],
    pub input_peak_meter: Arc<AtomicU32>,
    pub displayed_input_peak_level: f32,
    pub master_volume: Arc<AtomicU32>,
    pub limiter_is_active: Arc<AtomicBool>,
    pub limiter_threshold: Arc<AtomicU32>,
    pub limiter_release_mode: LfoRateMode,
    pub limiter_release_ms: Arc<AtomicU32>,
    pub limiter_release_sync_rate: Arc<AtomicU32>,
    pub gain_reduction_db: Arc<AtomicU32>,
    pub displayed_gain_reduction: f32,
    pub master_peak_meter: Arc<AtomicU32>,
    pub displayed_master_peak_level: f32,

    // --- Synth State ---
    pub engine_states: [EngineState; 2],
    pub synth_master_volume: Arc<AtomicU32>,
    pub synth_master_peak_meter: Arc<AtomicU32>,
    pub displayed_synth_master_peak_level: f32,

    // --- Sampler State ---
    pub sampler_volume: Arc<AtomicU32>,
    pub sampler_peak_meter: Arc<AtomicU32>,
    pub displayed_sampler_peak_level: f32,

    // --- 88 Keys Theory State ---
    pub theory_mode: TheoryMode,
    pub chord_display_mode: ChordDisplayMode,
    pub selected_scale: theory::Scale,
    pub selected_chord_style: theory::ChordStyle,
    pub available_chord_styles: Vec<(String, PathBuf)>,
    pub displayed_theory_notes: Vec<(u8, usize)>,
    pub last_recognized_chord_notes: BTreeSet<u8>,

    // --- Slicer State ---
    pub slicer_state: SlicerState,

    // --- MIDI Mapping State ---
    pub midi_mappings: Arc<RwLock<BTreeMap<MidiControlId, ControllableParameter>>>,
    pub midi_learn_target: Arc<RwLock<Option<ControllableParameter>>>,
    pub last_midi_cc_message: Arc<RwLock<Option<(MidiControlId, Instant)>>>,
    pub midi_mod_matrix_learn_target: Arc<RwLock<Option<(usize, usize)>>>,
    pub last_learned_mod_source: Arc<RwLock<Option<MidiControlId>>>,

    // --- Settings State (for UI) ---
    pub available_hosts: Vec<HostId>,
    pub selected_host_index: usize,
    pub midi_ports: Vec<(String, MidiInputPort)>,
    pub selected_midi_port_index: Option<usize>,
    pub selected_midi_channel: Arc<AtomicU8>,
    pub input_devices: Vec<(String, Device)>,
    pub output_devices: Vec<(String, Device)>,
    pub selected_input_device_index: Option<usize>,
    pub selected_output_device_index: Option<usize>,
    pub sample_rates: Vec<u32>,
    pub buffer_sizes: Vec<u32>,
    pub selected_sample_rate_index: usize,
    pub selected_buffer_size_index: usize,
    pub input_latency_compensation_ms: Arc<AtomicU32>,

    // --- Active Settings & Status ---
    pub active_input_device_name: Option<String>,
    pub active_output_device_name: Option<String>,
    pub active_sample_rate: u32,
    pub active_buffer_size: u32,
    pub audio_settings_status: Option<(String, Color32)>,
}

/// Helper struct to hold audio data and its original sample rate.
pub struct SourceAudio {
    pub sample_rate: u32,
    pub data: Vec<f32>,
}

// Helper function to load and convert a WAV file to mono f32 samples, retaining its original SR.
pub fn load_source_audio_file_with_sr(path: &Path) -> Result<SourceAudio> {
    let file = BufReader::new(File::open(path)?);
    let source = Decoder::new(file)?;

    let sample_rate = source.sample_rate();
    let num_channels = source.channels() as usize;
    // Rodio decoders often decode to i16. We must map and convert to f32.
    let all_samples: Vec<f32> = source.map(|s| s as f32 / i16::MAX as f32).collect();

    let data = if num_channels > 1 {
        all_samples
            .chunks_exact(num_channels)
            .map(|chunk| chunk.iter().sum::<f32>() / num_channels as f32)
            .collect()
    } else {
        all_samples
    };

    Ok(SourceAudio { sample_rate, data })
}

impl CypherApp {
    pub fn new(_cc: &eframe::CreationContext) -> Result<Self> {
        let settings = settings::load_settings();
        let theme = Theme::default();

        let available_hosts = cpal::available_hosts();
        let mut selected_host_index = available_hosts
            .iter()
            .position(|id| *id == cpal::default_host().id())
            .unwrap_or(0);
        if let Some(saved_host_name) = &settings.host_name {
            if let Some(i) = available_hosts
                .iter()
                .position(|id| id.name() == *saved_host_name)
            {
                selected_host_index = i;
            }
        }
        let initial_host_id = available_hosts[selected_host_index];
        let input_devices = audio_device::get_input_devices(initial_host_id)?;
        let output_devices = audio_device::get_output_devices(initial_host_id)?;

        let sample_rates = vec![44100, 48000, 88200, 96000];
        let buffer_sizes = vec![32, 64, 128, 256, 512, 1024, 2048];
        let selected_sample_rate_index = settings
            .sample_rate
            .as_ref()
            .and_then(|sr| sample_rates.iter().position(|&r| r == *sr))
            .unwrap_or(1);
        let selected_buffer_size_index = settings
            .buffer_size
            .as_ref()
            .and_then(|bs| buffer_sizes.iter().position(|&b| b == *bs))
            .unwrap_or(4);

        let track_mixer_state = Arc::new(RwLock::new(MixerState::default()));
        let peak_meters = Arc::new(std::array::from_fn(|_| AtomicU32::new(0)));
        let input_peak_meter = Arc::new(AtomicU32::new(0));
        let cpu_load = Arc::new(AtomicU32::new(0));
        let xrun_count = Arc::new(AtomicUsize::new(0));
        let input_latency_compensation_ms = Arc::new(AtomicU32::new(
            (settings.input_latency_compensation_ms * 100.0).round() as u32,
        ));

        let synth_master_volume = Arc::new(AtomicU32::new(1_000_000));
        let sampler_volume = Arc::new(AtomicU32::new(1_000_000));

        let master_volume = Arc::new(AtomicU32::new(1_000_000));
        let limiter_is_active = Arc::new(AtomicBool::new(true));
        let limiter_threshold = Arc::new(AtomicU32::new(1_000_000));
        let limiter_release_ms = Arc::new(AtomicU32::new(80_000));
        let limiter_release_sync_rate = Arc::new(AtomicU32::new(1_000_000));
        let gain_reduction_db = Arc::new(AtomicU32::new(0));
        let master_peak_meter = Arc::new(AtomicU32::new(0));
        let should_toggle_record_from_midi = Arc::new(AtomicBool::new(false));

        // **FIX**: Initialize the live MIDI map from the `settings` variable that was just loaded.
        let midi_mappings = Arc::new(RwLock::new(settings.midi_mappings.clone()));

        // Create the shared state for MIDI CC values
        let midi_cc_values = Arc::new(std::array::from_fn(|_| {
            std::array::from_fn(|_| AtomicU32::new(0))
        }));

        let app = Self {
            options_window_open: false,
            sample_pad_window_open: false,
            synth_editor_window_open: false,
            theme_editor_window_open: false,
            slicer_window_open: false,
            midi_mapping_window_open: false,
            is_recording_output: false,
            recording_notification: None,
            library_path: Vec::new(),
            library_view: LibraryView::Samples,
            asset_library: AssetLibrary::default(),
            theme,
            available_themes: Vec::new(),
            active_synth_section: [SynthUISection::Wavetable; 2],
            bpm_rounding_setting_changed_unapplied: false,
            current_session_path: None,
            _input_stream: None,
            _output_stream: None,
            _midi_connection: None,
            _command_thread_handle: None,
            command_sender: None,
            looper_states: Vec::new(),
            transport_playhead: Arc::new(AtomicUsize::new(0)),
            transport_len_samples: Arc::new(AtomicUsize::new(0)),
            transport_is_playing: Arc::new(AtomicBool::new(true)),
            synth_is_active: Arc::new(AtomicBool::new(false)),
            audio_input_is_armed: Arc::new(AtomicBool::new(false)),
            audio_input_is_monitored: Arc::new(AtomicBool::new(false)),
            sampler_is_active: Arc::new(AtomicBool::new(false)),
            sampler_pad_info: Default::default(),
            playing_pads: Arc::new(AtomicU16::new(0)),
            cpu_load,
            xrun_count,
            live_midi_notes: Arc::new(RwLock::new(BTreeSet::new())),
            should_toggle_record_from_midi,
            midi_cc_values,
            track_mixer_state,
            peak_meters,
            displayed_peak_levels: [0.0; NUM_LOOPERS],
            input_peak_meter,
            displayed_input_peak_level: 0.0,
            master_volume,
            limiter_is_active,
            limiter_threshold,
            limiter_release_mode: LfoRateMode::Hz,
            limiter_release_ms,
            limiter_release_sync_rate,
            gain_reduction_db,
            displayed_gain_reduction: 0.0,
            master_peak_meter,
            displayed_master_peak_level: 0.0,
            engine_states: [EngineState::new_wavetable(), EngineState::new_wavetable()],
            synth_master_volume,
            synth_master_peak_meter: Arc::new(AtomicU32::new(0)),
            displayed_synth_master_peak_level: 0.0,
            sampler_volume,
            sampler_peak_meter: Arc::new(AtomicU32::new(0)),
            displayed_sampler_peak_level: 0.0,
            theory_mode: TheoryMode::Scales,
            chord_display_mode: ChordDisplayMode::Stacked,
            selected_scale: Scale::Ionian,
            selected_chord_style: ChordStyle::default(),
            available_chord_styles: Vec::new(),
            displayed_theory_notes: Vec::new(),
            last_recognized_chord_notes: BTreeSet::new(),
            slicer_state: SlicerState::new(),
            midi_mappings,
            midi_learn_target: Arc::new(RwLock::new(None)),
            last_midi_cc_message: Arc::new(RwLock::new(None)),
            midi_mod_matrix_learn_target: Arc::new(RwLock::new(None)),
            last_learned_mod_source: Arc::new(RwLock::new(None)),
            available_hosts,
            selected_host_index,
            midi_ports: midi::get_midi_ports()?,
            selected_midi_port_index: None,
            selected_midi_channel: Arc::new(AtomicU8::new(settings.midi_channel)),
            input_devices,
            output_devices,
            selected_input_device_index: None,
            selected_output_device_index: None,
            sample_rates,
            buffer_sizes,
            selected_sample_rate_index,
            selected_buffer_size_index,
            input_latency_compensation_ms,
            active_input_device_name: None,
            active_output_device_name: None,
            active_sample_rate: 0,
            active_buffer_size: 0,
            audio_settings_status: None,
            settings,
        };

        CypherApp::post_new(app)
    }

    pub fn is_all_muted(&self) -> bool {
        if let Ok(mixer_state) = self.track_mixer_state.read() {
            mixer_state.tracks.iter().all(|track| track.is_muted)
        } else {
            false // Default to not muted if lock fails
        }
    }

    pub fn toggle_mute_all(&mut self) {
        if let Ok(mut mixer_state) = self.track_mixer_state.write() {
            // If any track is NOT muted, then the action is to mute all.
            // Otherwise, the action is to unmute all.
            let should_mute_all = mixer_state.tracks.iter().any(|track| !track.is_muted);
            for track in mixer_state.tracks.iter_mut() {
                track.is_muted = should_mute_all;
            }
        }
    }

    pub fn post_new(mut app: Self) -> Result<Self> {
        app.rescan_asset_library();
        app.rescan_available_themes();
        app.rescan_chord_styles();

        // Load first available chord style if none is selected
        let style_to_load = app.available_chord_styles.first().map(|(_, path)| path.clone());
        if let Some(path) = style_to_load {
            app.load_chord_style(&path);
        }

        app.selected_input_device_index = app
            .settings
            .input_device
            .as_ref()
            .and_then(|name| app.input_devices.iter().position(|(d_name, _)| d_name == name));
        app.selected_output_device_index = app
            .settings
            .output_device
            .as_ref()
            .and_then(|name| app.output_devices.iter().position(|(d_name, _)| d_name == name));
        app.selected_midi_port_index = app
            .settings
            .midi_port_name
            .as_ref()
            .and_then(|name| app.midi_ports.iter().position(|(p_name, _)| p_name == name));
        if app.selected_midi_port_index.is_none() && !app.midi_ports.is_empty() {
            app.selected_midi_port_index = Some(0);
        }

        if let Err(e) = app.start_audio() {
            app.audio_settings_status =
                Some((format!("Failed to auto-start audio: {}", e), Color32::YELLOW));
        } else {
            app.audio_settings_status =
                Some(("Audio engine running.".to_string(), Color32::GREEN));
        }

        if let Some(path) = app.settings.last_sampler_kit.clone() {
            app.load_kit(&path);
        }
        if let Some(path) = app.settings.last_synth_preset.clone() {
            app.load_preset_from_path(&path);
        }
        if let Some(path) = app.settings.last_theme.clone() {
            app.load_theme_from_path(&path);
        }

        Ok(app)
    }

    pub fn load_wav_for_synth_slot(
        &mut self,
        engine_index: usize,
        slot_index: usize,
        path: PathBuf,
    ) {
        if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
            if let Ok(source_audio) = load_source_audio_file_with_sr(&path) {
                if let EngineState::Wavetable(wt_state) = &mut self.engine_states[engine_index] {
                    wt_state.wavetable_names[slot_index] = name.to_string();
                    wt_state.wavetable_sources[slot_index] = WavetableSource::File(path);
                    wt_state.window_positions[slot_index] = 0.0;
                    wt_state.original_sources[slot_index] = Arc::new(source_audio.data);
                    wt_state.source_sample_rates[slot_index] = source_audio.sample_rate;
                    wt_state.force_redraw_generation += 1; // Invalidate visualizer cache

                    // Now, generate and send the initial wavetable slice
                    self.generate_and_send_wavetable(engine_index, slot_index, 0.0);
                }
            }
        }
    }

    pub fn load_sample_for_sampler_slot(
        &mut self,
        engine_index: usize,
        slot_index: usize,
        path: PathBuf,
    ) {
        if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
            match self.load_and_resample_wav_file(&path, self.active_sample_rate as f32) {
                Ok(audio_data) => {
                    if let EngineState::Sampler(sampler_state) =
                        &mut self.engine_states[engine_index]
                    {
                        // Update UI state
                        sampler_state.sample_names[slot_index] = name.to_string();
                        sampler_state.sample_paths[slot_index] = Some(path.clone());
                        *sampler_state.sample_data_for_ui[slot_index].write().unwrap() =
                            audio_data.clone();
                        // Bust the visualizer cache
                        sampler_state.force_redraw_generation += 1;

                        // Send command to audio thread
                        self.send_command(AudioCommand::LoadSampleForSamplerSlot {
                            engine_index,
                            slot_index,
                            audio_data: Arc::new(audio_data),
                        });
                    }
                }
                Err(e) => {
                    eprintln!("Error loading sample {}: {}", path.display(), e);
                }
            }
        }
    }

    fn resolve_path(&self, path_to_resolve: &Path) -> Option<PathBuf> {
        if path_to_resolve.exists() {
            return Some(path_to_resolve.to_path_buf());
        }
        if let Some(config_dir) = settings::get_config_dir() {
            let relative_path = config_dir.join(path_to_resolve);
            if relative_path.exists() {
                return Some(relative_path);
            }
            let path_str = path_to_resolve.to_string_lossy();
            let filename_str = path_str.rsplit(|c| c == '/' || c == '\\').next();
            if let Some(filename) = filename_str {
                for entry in WalkDir::new(&config_dir).into_iter().filter_map(|e| e.ok()) {
                    if entry.file_name() == filename {
                        return Some(entry.path().to_path_buf());
                    }
                }
            }
        }
        None
    }

    pub fn on_host_changed(&mut self) {
        let host_id = self.available_hosts[self.selected_host_index];
        match audio_device::get_input_devices(host_id) {
            Ok(devices) => self.input_devices = devices,
            Err(e) => {
                eprintln!(
                    "Error getting input devices for host {}: {}",
                    host_id.name(),
                    e
                );
                self.input_devices.clear();
            }
        }
        match audio_device::get_output_devices(host_id) {
            Ok(devices) => self.output_devices = devices,
            Err(e) => {
                eprintln!(
                    "Error getting output devices for host {}: {}",
                    host_id.name(),
                    e
                );
                self.output_devices.clear();
            }
        }
        self.selected_input_device_index = None;
        self.selected_output_device_index = None;
    }

    pub fn rescan_asset_library(&mut self) {
        self.asset_library.clear();
        if let Some(config_dir) = settings::get_config_dir() {
            let samples_dir = config_dir.join("Samples");
            let presets_dir = config_dir.join("SynthPresets");
            let kits_dir = config_dir.join("Kits");
            let sessions_dir = config_dir.join("Sessions");

            for entry in WalkDir::new(&samples_dir)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file()
                    && entry.path().extension().map_or(false, |e| e == "wav")
                {
                    if let Ok(relative_path) = entry.path().strip_prefix(&samples_dir) {
                        let segments: Vec<String> = relative_path
                            .iter()
                            .map(|s| s.to_string_lossy().to_string())
                            .collect();
                        if let Some(sample_ref) = SampleRef::new(entry.path().to_path_buf()) {
                            self.asset_library
                                .sample_root
                                .insert_asset(&segments, Asset::Sample(sample_ref));
                        }
                    }
                }
            }
            for entry in WalkDir::new(&presets_dir)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file()
                    && entry.path().extension().map_or(false, |e| e == "json")
                {
                    if let Ok(relative_path) = entry.path().strip_prefix(&presets_dir) {
                        let segments: Vec<String> = relative_path
                            .iter()
                            .map(|s| s.to_string_lossy().to_string())
                            .collect();
                        if let Some(preset_ref) = SynthPresetRef::new(entry.path().to_path_buf()) {
                            self.asset_library
                                .synth_root
                                .insert_asset(&segments, Asset::SynthPreset(preset_ref));
                        }
                    }
                }
            }
            for entry in WalkDir::new(&kits_dir).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file()
                    && entry.path().extension().map_or(false, |e| e == "json")
                {
                    if let Ok(relative_path) = entry.path().strip_prefix(&kits_dir) {
                        let segments: Vec<String> = relative_path
                            .iter()
                            .map(|s| s.to_string_lossy().to_string())
                            .collect();
                        if let Some(kit_ref) = SamplerKitRef::new(entry.path().to_path_buf()) {
                            self.asset_library
                                .kit_root
                                .insert_asset(&segments, Asset::SamplerKit(kit_ref));
                        }
                    }
                }
            }
            if sessions_dir.is_dir() {
                for entry in WalkDir::new(&sessions_dir).min_depth(1).max_depth(1).into_iter().filter_map(|e| e.ok()) {
                    if entry.file_type().is_dir() {
                        let segments = vec![entry.file_name().to_string_lossy().to_string()];
                        if let Some(session_ref) = SessionRef::new(entry.path().to_path_buf()) {
                            self.asset_library
                                .session_root
                                .insert_asset(&segments, Asset::Session(session_ref));
                        }
                    }
                }
            }
        }
    }

    pub fn rescan_available_themes(&mut self) {
        self.available_themes.clear();
        if let Some(config_dir) = settings::get_config_dir() {
            let themes_dir = config_dir.join("Themes");
            if themes_dir.is_dir() {
                for entry in WalkDir::new(themes_dir)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    if entry.file_type().is_file()
                        && entry.path().extension().map_or(false, |e| e == "json")
                    {
                        if let Some(name) = entry.path().file_stem().and_then(|s| s.to_str()) {
                            self.available_themes
                                .push((name.to_string(), entry.path().to_path_buf()));
                        }
                    }
                }
            }
        }
        // Sort alphabetically by name
        self.available_themes.sort_by(|a, b| a.0.cmp(&b.0));
    }

    pub fn rescan_chord_styles(&mut self) {
        self.available_chord_styles.clear();
        if let Some(config_dir) = settings::get_config_dir() {
            let styles_dir = config_dir.join("ChordStyles");
            if !styles_dir.exists() {
                fs::create_dir_all(&styles_dir).ok();
                // Create a default style file if one doesn't exist
                let default_style_path = styles_dir.join("Pop Triads.json");
                if !default_style_path.exists() {
                    let mut default_suggestions = BTreeMap::new();
                    default_suggestions.insert("dominant".to_string(), theory::ChordQuality::MajorTriad);
                    default_suggestions.insert("subdominant".to_string(), theory::ChordQuality::MajorTriad);
                    default_suggestions.insert("relative_minor".to_string(), theory::ChordQuality::MinorTriad);
                    default_suggestions.insert("relative_major".to_string(), theory::ChordQuality::MajorTriad);
                    default_suggestions.insert("dominant_of_dominant".to_string(), theory::ChordQuality::MajorTriad);
                    let default_style = ChordStyle {
                        name: "Pop Triads".to_string(),
                        suggestions: default_suggestions,
                    };
                    if let Ok(json) = serde_json::to_string_pretty(&default_style) {
                        fs::write(default_style_path, json).ok();
                    }
                }
            }

            if styles_dir.is_dir() {
                for entry in WalkDir::new(styles_dir).into_iter().filter_map(|e| e.ok()) {
                    if entry.file_type().is_file() && entry.path().extension().map_or(false, |e| e == "json") {
                        if let Ok(json_string) = fs::read_to_string(entry.path()) {
                            if let Ok(style) = serde_json::from_str::<ChordStyle>(&json_string) {
                                self.available_chord_styles.push((style.name, entry.path().to_path_buf()));
                            }
                        }
                    }
                }
            }
        }
        self.available_chord_styles.sort_by(|a, b| a.0.cmp(&b.0));
    }

    pub fn load_chord_style(&mut self, path: &Path) {
        if let Ok(json_string) = fs::read_to_string(path) {
            if let Ok(style) = serde_json::from_str::<ChordStyle>(&json_string) {
                self.selected_chord_style = style;
            }
        }
    }

    pub fn stop_audio(&mut self) {
        self._midi_connection.take();
        self.command_sender.take();
        if let Some(handle) = self._command_thread_handle.take() {
            if let Err(e) = handle.join() {
                eprintln!("Error joining command thread: {:?}", e);
            }
        }
        self._input_stream.take();
        self._output_stream.take();
        println!("Audio engine stopped.");
    }

    pub fn start_audio(&mut self) -> Result<()> {
        let host_id = self.available_hosts[self.selected_host_index];
        let input_device = self
            .selected_input_device_index
            .and_then(|i| self.input_devices.get(i).cloned());
        let output_device = self
            .selected_output_device_index
            .and_then(|i| self.output_devices.get(i).cloned());
        let input_device_name = input_device.as_ref().map(|(name, _)| name.clone());
        let output_device_name = output_device.as_ref().map(|(name, _)| name.clone());
        let sample_rate = self
            .sample_rates
            .get(self.selected_sample_rate_index)
            .copied();
        let buffer_size = self
            .buffer_sizes
            .get(self.selected_buffer_size_index)
            .copied();
        let (mpsc_sender, mpsc_receiver) = mpsc::channel::<AudioCommand>();
        let command_rb = HeapRb::<AudioCommand>::new(256);
        let (mut ringbuf_producer, ringbuf_consumer) = command_rb.split();
        let audio_rb = HeapRb::<f32>::new((sample_rate.unwrap_or(48000) * 4) as usize);
        let (audio_producer, audio_consumer) = audio_rb.split();

        self._command_thread_handle = Some(thread::spawn(move || {
            while let Ok(command) = mpsc_receiver.recv() {
                if ringbuf_producer.push(command).is_err() {
                    eprintln!("Command ringbuffer full. Command dropped.");
                }
            }
        }));

        let engine_params = [self.get_engine_params(0), self.get_engine_params(1)];

        let (mut engine, looper_states) = AudioEngine::new(
            ringbuf_consumer,
            audio_consumer,
            sample_rate.unwrap_or(48000) as f32,
            self.selected_midi_channel.clone(),
            self.playing_pads.clone(),
            self.track_mixer_state.clone(),
            self.peak_meters.clone(),
            self.cpu_load.clone(),
            self.input_peak_meter.clone(),
            self.audio_input_is_armed.clone(),
            self.audio_input_is_monitored.clone(),
            self.input_latency_compensation_ms.clone(),
            self.sampler_volume.clone(),
            self.sampler_peak_meter.clone(),
            self.master_volume.clone(),
            self.limiter_is_active.clone(),
            self.limiter_threshold.clone(),
            self.limiter_release_ms.clone(),
            self.limiter_release_sync_rate.clone(),
            self.gain_reduction_db.clone(),
            self.master_peak_meter.clone(),
            self.synth_master_volume.clone(),
            self.synth_master_peak_meter.clone(),
            engine_params,
            self.settings.bpm_rounding,
            self.transport_is_playing.clone(),
            self.should_toggle_record_from_midi.clone(),
            self.midi_cc_values.clone(),
        );
        self.looper_states = looper_states;
        self.transport_playhead = engine.transport_playhead.clone();
        self.transport_len_samples = engine.transport_len_samples.clone();
        self.transport_is_playing = engine.transport_is_playing.clone();
        self.synth_is_active = engine.synth_is_active.clone();
        self.audio_input_is_armed = engine.audio_input_is_armed.clone();
        self.audio_input_is_monitored = engine.audio_input_is_monitored.clone();
        self.sampler_is_active = engine.sampler_is_active.clone();
        self.should_toggle_record_from_midi = engine.should_toggle_record.clone();
        self.midi_cc_values = engine.midi_cc_values.clone();

        let (input_stream, output_stream, active_sr, active_bs) =
            audio_io::init_and_run_streams(
                host_id,
                input_device_name.clone(),
                output_device_name.clone(),
                sample_rate,
                buffer_size,
                audio_producer,
                engine,
                self.xrun_count.clone(),
            )?;

        self._input_stream = Some(input_stream);
        self._output_stream = Some(output_stream);
        self.command_sender = Some(mpsc_sender);
        self.active_sample_rate = active_sr;
        self.active_buffer_size = active_bs;
        self.active_input_device_name = input_device_name;
        self.active_output_device_name = output_device_name;

        self.reconnect_midi()?;
        Ok(())
    }

    fn get_engine_params(&self, index: usize) -> EngineWithVolumeAndPeak {
        match &self.engine_states[index] {
            EngineState::Wavetable(state) => {
                let params = WavetableParams(
                    state.wavetable_set.clone(),
                    state.wavetable_position.clone(),
                    state.filter_settings.clone(),
                    state.wavetable_mixer_settings.clone(),
                    state.lfo_settings.clone(),
                    state.lfo2_settings.clone(),
                    state.mod_matrix.clone(),
                    state.saturation_settings.clone(),
                    state.lfo_value_atomic.clone(),
                    state.lfo2_value_atomic.clone(),
                    state.env2_value_atomic.clone(),
                    state.pitch_mod_atomic.clone(),
                    state.bell_pos_atomic.clone(),
                    state.bell_amount_atomic.clone(),
                    state.bell_width_atomic.clone(),
                    state.saturation_mod_atomic.clone(),
                    state.final_wt_pos_atomic.clone(),
                    state.final_cutoff_atomic.clone(),
                );
                (
                    state.volume.clone(),
                    state.peak_meter.clone(),
                    EngineParamsUnion::Wavetable(params),
                )
            }
            EngineState::Sampler(state) => {
                let params = SamplerParams(
                    state.filter_settings.clone(),
                    state.lfo_settings.clone(),
                    state.lfo2_settings.clone(),
                    state.mod_matrix.clone(),
                    state.saturation_settings.clone(),
                    state.lfo_value_atomic.clone(),
                    state.lfo2_value_atomic.clone(),
                    state.env2_value_atomic.clone(),
                    state.pitch_mod_atomic.clone(),
                    state.amp_mod_atomic.clone(),
                    state.cutoff_mod_atomic.clone(),
                    state.saturation_mod_atomic.clone(),
                    state.final_cutoff_atomic.clone(),
                    state.last_triggered_slot_index.clone(),
                );
                (
                    state.volume.clone(),
                    state.peak_meter.clone(),
                    EngineParamsUnion::Sampler(params),
                )
            }
        }
    }

    pub fn apply_audio_settings(&mut self) {
        self.audio_settings_status = None;
        let was_sampler_active = self.sampler_is_active.load(Ordering::Relaxed);
        let was_synth_active = self.synth_is_active.load(Ordering::Relaxed);

        let old_config = (
            self.selected_host_index,
            self.selected_input_device_index,
            self.selected_output_device_index,
            self.selected_sample_rate_index,
            self.selected_buffer_size_index,
        );
        self.stop_audio();

        if let Err(e) = self.start_audio() {
            self.audio_settings_status =
                Some((format!("Failed to apply settings: {}", e), Color32::RED));
            println!("Attempting to revert to last known working audio configuration...");
            (
                self.selected_host_index,
                self.selected_input_device_index,
                self.selected_output_device_index,
                self.selected_sample_rate_index,
                self.selected_buffer_size_index,
            ) = old_config;
            if let Err(revert_err) = self.start_audio() {
                self.audio_settings_status = Some((
                    format!("FATAL: Could not restore previous settings: {}", revert_err),
                    Color32::RED,
                ));
                println!("FATAL: Revert failed. Audio engine is stopped.");
            } else {
                println!("Successfully reverted to previous audio settings.");
            }
        } else {
            self.audio_settings_status =
                Some(("Audio settings applied successfully.".to_string(), Color32::GREEN));
            self.save_settings();
        }

        if was_sampler_active {
            self.send_command(AudioCommand::ActivateSampler);
        }
        if was_synth_active {
            self.send_command(AudioCommand::ActivateSynth);
        }
    }

    pub fn reconnect_midi(&mut self) -> Result<()> {
        self._midi_connection = None;
        if let (Some(index), Some(sender)) =
            (self.selected_midi_port_index, self.command_sender.as_ref())
        {
            if let Some(port_info) = self.midi_ports.get(index) {
                self._midi_connection = Some(midi::connect_midi(
                    sender.clone(),
                    self.live_midi_notes.clone(),
                    port_info.1.clone(),
                    self.midi_mappings.clone(),
                    self.midi_learn_target.clone(),
                    self.last_midi_cc_message.clone(),
                    self.midi_cc_values.clone(),
                    self.midi_mod_matrix_learn_target.clone(),
                    self.last_learned_mod_source.clone(),
                )?);
            }
        }
        Ok(())
    }

    pub fn save_settings(&mut self) {
        self.settings.host_name = self
            .available_hosts
            .get(self.selected_host_index)
            .map(|id| id.name().to_string());
        self.settings.midi_port_name = self
            .selected_midi_port_index
            .and_then(|index| self.midi_ports.get(index))
            .map(|(name, _)| name.clone());
        self.settings.input_device = self
            .selected_input_device_index
            .and_then(|index| self.input_devices.get(index))
            .map(|(name, _)| name.clone());
        self.settings.output_device = self
            .selected_output_device_index
            .and_then(|index| self.output_devices.get(index))
            .map(|(name, _)| name.clone());
        self.settings.sample_rate = self
            .sample_rates
            .get(self.selected_sample_rate_index)
            .copied();
        self.settings.buffer_size = self
            .buffer_sizes
            .get(self.selected_buffer_size_index)
            .copied();
        self.settings.midi_channel = self.selected_midi_channel.load(Ordering::Relaxed);
        self.settings.input_latency_compensation_ms =
            self.input_latency_compensation_ms.load(Ordering::Relaxed) as f32 / 100.0;

        // **FIX**: The live BTreeMap is now copied to the serializable Vec inside the function we call.
        self.settings.midi_mappings = self.midi_mappings.read().unwrap().clone();

        // **THE FIX IS HERE**: Pass a mutable reference.
        settings::save_settings(&mut self.settings);
        self.bpm_rounding_setting_changed_unapplied = false;
    }

    pub fn send_command(&self, command: AudioCommand) {
        if let Some(sender) = &self.command_sender {
            if let Err(e) = sender.send(command) {
                eprintln!("Failed to send command: {}. Audio thread may be offline.", e);
            }
        }
    }

    fn load_and_resample_wav_file(&self, path: &Path, target_sr: f32) -> Result<Vec<f32>> {
        let file = BufReader::new(File::open(path)?);
        let source = Decoder::new(file)?;
        let source_sr = source.sample_rate() as f32;
        let num_channels = source.channels() as usize;

        let all_samples: Vec<f32> = source.map(|s| s as f32 / i16::MAX as f32).collect();

        let mono_samples = if num_channels > 1 {
            all_samples
                .chunks_exact(num_channels)
                .map(|chunk| chunk.iter().sum::<f32>() / num_channels as f32)
                .collect()
        } else {
            all_samples
        };

        if (source_sr - target_sr).abs() > 1e-3 {
            println!(
                "Resampling sample from {} Hz to {} Hz",
                source_sr, target_sr
            );
            let params = SincInterpolationParameters {
                sinc_len: 256,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 256,
                window: WindowFunction::BlackmanHarris2,
            };
            let mut resampler = SincFixedIn::<f32>::new(
                target_sr as f64 / source_sr as f64,
                2.0,
                params,
                mono_samples.len(),
                1,
            )?;
            let waves_in = vec![mono_samples];
            let waves_out = resampler.process(&waves_in, None)?;
            Ok(waves_out.into_iter().next().unwrap_or_default())
        } else {
            Ok(mono_samples)
        }
    }

    pub fn load_sample_for_pad(&mut self, pad_index: usize, sample_ref: SampleRef) {
        match self.load_and_resample_wav_file(&sample_ref.path, self.active_sample_rate as f32) {
            Ok(audio_data) => {
                self.send_command(AudioCommand::LoadSamplerSample {
                    pad_index,
                    audio_data: Arc::new(audio_data),
                });
                self.sampler_pad_info[pad_index] = Some(sample_ref);
            }
            Err(e) => {
                eprintln!(
                    "Failed to load and resample sample '{}': {}",
                    sample_ref.path.display(),
                    e
                );
            }
        }
    }
    

    pub fn load_kit(&mut self, path: &PathBuf) {
        // --- FIX START: Resolve the path first to handle both absolute and relative inputs ---
        let absolute_path = if path.is_absolute() {
            path.clone()
        } else if let Some(config_dir) = settings::get_config_dir() {
            config_dir.join(path)
        } else {
            path.clone() // Fallback
        };
        // --- FIX END ---

        if let Ok(json_string) = fs::read_to_string(&absolute_path) {
            if let Ok(kit) = serde_json::from_str::<SamplerKit>(&json_string) {
                for (i, pad_path) in kit.pads.into_iter().enumerate() {
                    if let Some(p) = pad_path {
                        if let Some(resolved_path) = self.resolve_path(&p) {
                            if let Some(sample) = SampleRef::new(resolved_path.clone()) {
                                self.load_sample_for_pad(i, sample);
                            }
                        } else {
                            eprintln!("Sample path not found for kit: {}", p.display());
                            self.sampler_pad_info[i] = None;
                            self.send_command(AudioCommand::ClearSample { pad_index: i });
                        }
                    } else {
                        self.sampler_pad_info[i] = None;
                        self.send_command(AudioCommand::ClearSample { pad_index: i });
                    }
                }

                // Convert the kit path to be relative for portability before saving.
                if let Some(config_dir) = settings::get_config_dir() {
                    if let Ok(relative_path) = absolute_path.strip_prefix(&config_dir) {
                        // Success: store the portable, relative path.
                        self.settings.last_sampler_kit = Some(relative_path.to_path_buf());
                    } else {
                        // Fallback: the kit is outside the portable folder, store its absolute path.
                        self.settings.last_sampler_kit = Some(absolute_path);
                    }
                } else {
                    // Fallback: can't get config dir, store absolute path.
                    self.settings.last_sampler_kit = Some(absolute_path);
                }
            }
        } else {
            eprintln!("Failed to read kit file: {}", absolute_path.display());
        }
    }

    pub fn load_preset_from_path(&mut self, path: &Path) {
        // --- Step 1: Resolve the incoming path to an absolute one for reading ---
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(config_dir) = settings::get_config_dir() {
            config_dir.join(path)
        } else {
            path.to_path_buf() // Fallback
        };

        if !absolute_path.exists() {
            eprintln!("Preset file not found: {}", absolute_path.display());
            return;
        }

        // --- Step 2: Read the file and parse the preset ---
        if let Ok(json_string) = fs::read_to_string(&absolute_path) {
            if let Ok(preset) = serde_json::from_str::<SynthPreset>(&json_string) {
                let mut commands_to_send = Vec::new();
                let mut sampler_loads_to_perform = Vec::new();

                // --- Three-pass loading to avoid borrow checker issues ---
                // Pass 1: Load all raw audio data immutably.
                let mut loaded_wavetables = Vec::new();
                for i in 0..2 {
                    if let SynthEnginePreset::Wavetable(engine_preset) = &preset.engine_presets[i]
                    {
                        for k in 0..4 {
                            if let WavetableSource::File(p) = &engine_preset.wavetable_sources[k]
                            {
                                if let Some(resolved_path) = self.resolve_path(p) {
                                    if let Ok(source_audio) =
                                        load_source_audio_file_with_sr(&resolved_path)
                                    {
                                        loaded_wavetables.push((i, k, resolved_path, source_audio));
                                    }
                                }
                            }
                        }
                    }
                }

                // Pass 2: Mutably update all state.
                for i in 0..2 {
                    let needs_engine_change =
                        match (&preset.engine_presets[i], &self.engine_states[i]) {
                            (SynthEnginePreset::Wavetable(_), EngineState::Sampler(_)) => true,
                            (SynthEnginePreset::Sampler(_), EngineState::Wavetable(_)) => true,
                            _ => false,
                        };

                    if needs_engine_change {
                        let is_wavetable =
                            matches!(preset.engine_presets[i], SynthEnginePreset::Wavetable(_));
                        self.set_engine_type(i, is_wavetable);
                    }

                    match &preset.engine_presets[i] {
                        SynthEnginePreset::Wavetable(engine_preset) => {
                            if let EngineState::Wavetable(wt_state) = &mut self.engine_states[i] {
                                // Apply non-wavetable settings
                                wt_state.volume.store(
                                    (engine_preset.volume * 1_000_000.0) as u32,
                                    Ordering::Relaxed,
                                );
                                *wt_state.saturation_settings.write().unwrap() =
                                    engine_preset.saturation_settings;
                                wt_state.amp_adsr = engine_preset.amp_adsr;
                                wt_state.filter_adsr = engine_preset.filter_adsr;
                                *wt_state.filter_settings.write().unwrap() = engine_preset.filter;
                                *wt_state.lfo_settings.write().unwrap() =
                                    engine_preset.lfo_settings;
                                *wt_state.lfo2_settings.write().unwrap() =
                                    engine_preset.lfo2_settings;
                                *wt_state.mod_matrix.write().unwrap() =
                                    engine_preset.mod_matrix.clone();
                                wt_state.is_polyphonic = engine_preset.is_polyphonic;
                                wt_state.wavetable_position.store(
                                    engine_preset.wavetable_position_m_u32,
                                    Ordering::Relaxed,
                                );
                                *wt_state.wavetable_mixer_settings.write().unwrap() =
                                    engine_preset.wavetable_mixer;

                                // Queue commands for settings
                                commands_to_send
                                    .push(AudioCommand::SetAmpAdsr(i, engine_preset.amp_adsr));
                                commands_to_send
                                    .push(AudioCommand::SetFilterAdsr(i, engine_preset.filter_adsr));
                                commands_to_send
                                    .push(AudioCommand::SetSynthMode(i, engine_preset.is_polyphonic));

                                // Store the pre-loaded raw audio data
                                for (loaded_i, loaded_k, resolved_path, source_audio) in
                                    &loaded_wavetables
                                {
                                    if *loaded_i == i {
                                        wt_state.wavetable_names[*loaded_k] = resolved_path
                                            .file_stem()
                                            .unwrap()
                                            .to_string_lossy()
                                            .to_string();
                                        wt_state.wavetable_sources[*loaded_k] =
                                            WavetableSource::File(resolved_path.clone());
                                        wt_state.window_positions[*loaded_k] =
                                            engine_preset.window_positions[*loaded_k];
                                        wt_state.original_sources[*loaded_k] =
                                            Arc::new(source_audio.data.clone());
                                        wt_state.source_sample_rates[*loaded_k] =
                                            source_audio.sample_rate;
                                    }
                                }
                            }
                        }
                        SynthEnginePreset::Sampler(engine_preset) => {
                            if let EngineState::Sampler(sampler_state) = &mut self.engine_states[i]
                            {
                                // Apply global settings
                                sampler_state.volume.store(
                                    (engine_preset.volume * 1_000_000.0) as u32,
                                    Ordering::Relaxed,
                                );
                                *sampler_state.saturation_settings.write().unwrap() =
                                    engine_preset.saturation_settings;
                                sampler_state.amp_adsr = engine_preset.amp_adsr;
                                sampler_state.filter_adsr = engine_preset.filter_adsr;
                                *sampler_state.filter_settings.write().unwrap() =
                                    engine_preset.filter;
                                *sampler_state.lfo_settings.write().unwrap() =
                                    engine_preset.lfo_settings;
                                *sampler_state.lfo2_settings.write().unwrap() =
                                    engine_preset.lfo2_settings;
                                *sampler_state.mod_matrix.write().unwrap() =
                                    engine_preset.mod_matrix.clone();
                                sampler_state.is_polyphonic = engine_preset.is_polyphonic;

                                // Sampler specifics
                                sampler_state.root_notes = engine_preset.root_notes;
                                sampler_state.global_fine_tune_cents =
                                    engine_preset.global_fine_tune_cents;
                                sampler_state.fade_out = engine_preset.fade_out;

                                // Queue commands
                                commands_to_send
                                    .push(AudioCommand::SetAmpAdsr(i, engine_preset.amp_adsr));
                                commands_to_send.push(AudioCommand::SetFilterAdsr(
                                    i,
                                    engine_preset.filter_adsr,
                                ));
                                commands_to_send.push(AudioCommand::SetSynthMode(
                                    i,
                                    engine_preset.is_polyphonic,
                                ));
                                commands_to_send.push(AudioCommand::SetSamplerSettings {
                                    engine_index: i,
                                    root_notes: engine_preset.root_notes,
                                    global_fine_tune_cents: engine_preset.global_fine_tune_cents,
                                    fade_out: engine_preset.fade_out,
                                });

                                // Clear all slots before loading new ones
                                for k in 0..NUM_SAMPLE_SLOTS {
                                    sampler_state.sample_names[k] = "Empty".to_string();
                                    sampler_state.sample_paths[k] = None;
                                    sampler_state.sample_data_for_ui[k].write().unwrap().clear();
                                }

                                // Defer the actual loading until after this loop
                                for (k, path_opt) in engine_preset.sample_paths.iter().enumerate()
                                {
                                    if let Some(p) = path_opt {
                                        if let Some(resolved_path) = self.resolve_path(p) {
                                            sampler_loads_to_perform
                                                .push((i, k, resolved_path.clone()));
                                        } else {
                                            eprintln!("Sample file not found: {:?}", p);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Pass 3: Generate and send wavetables now that state is updated.
                let default_tables = wavetable_engine::WavetableSet::new_basic();
                for i in 0..2 {
                    if let SynthEnginePreset::Wavetable(engine_preset) = &preset.engine_presets[i]
                    {
                        for k in 0..4 {
                            match &engine_preset.wavetable_sources[k] {
                                WavetableSource::File(_) => {
                                    // We only need to generate for files that were actually found and loaded
                                    if loaded_wavetables.iter().any(|(li, lk, _, _)| *li == i && *lk == k) {
                                        self.generate_and_send_wavetable(
                                            i,
                                            k,
                                            engine_preset.window_positions[k],
                                        );
                                    }
                                }
                                WavetableSource::Default(name) => {
                                    let (default_name, default_audio) = default_tables
                                        .tables
                                        .iter()
                                        .find(|t| &t.name == name)
                                        .map(|t| (t.name.clone(), t.table.clone()))
                                        .unwrap_or_else(|| {
                                            (
                                                "Sine".to_string(),
                                                default_tables.tables[0].table.clone(),
                                            )
                                        });
                                    commands_to_send.push(AudioCommand::SetWavetable {
                                        engine_index: i,
                                        slot_index: k,
                                        audio_data: Arc::new(default_audio),
                                        name: default_name,
                                    });
                                }
                            }
                        }
                    }
                }

                // Send all commands and perform sample loads
                for cmd in commands_to_send {
                    self.send_command(cmd);
                }
                for (engine_idx, slot_idx, p) in sampler_loads_to_perform {
                    self.load_sample_for_sampler_slot(engine_idx, slot_idx, p);
                }

                // --- Step 3: THE FIX - Store a relative path if possible ---
                if let Some(config_dir) = settings::get_config_dir() {
                    if let Ok(relative_path) = absolute_path.strip_prefix(&config_dir) {
                        // Success: store the portable, relative path.
                        self.settings.last_synth_preset = Some(relative_path.to_path_buf());
                    } else {
                        // Fallback: the preset is outside the portable folder, store its absolute path.
                        self.settings.last_synth_preset = Some(absolute_path);
                    }
                } else {
                    // Fallback: can't get config dir, store absolute path.
                    self.settings.last_synth_preset = Some(absolute_path);
                }
            }
        }
    }

    pub fn save_preset(&mut self) {
        if let Some(config_dir) = settings::get_config_dir() {
            let presets_dir = config_dir.join("SynthPresets");
            if let Some(path) = FileDialog::new()
                .add_filter("json", &["json"])
                .set_directory(&presets_dir)
                .save_file()
            {
                let engine_presets = [
                    self.create_engine_preset(0, &config_dir),
                    self.create_engine_preset(1, &config_dir),
                ];

                let preset = SynthPreset { engine_presets };

                if let Ok(json) = serde_json::to_string_pretty(&preset) {
                    if let Err(e) = fs::write(&path, json) {
                        eprintln!("Failed to save synth preset: {}", e);
                    } else {
                        self.settings.last_synth_preset = Some(path);
                    }
                }
            }
        }
    }

    fn create_engine_preset(&self, index: usize, config_dir: &Path) -> SynthEnginePreset {
        match &self.engine_states[index] {
            EngineState::Wavetable(state) => {
                let mut sources = state.wavetable_sources.clone();
                for source in sources.iter_mut() {
                    if let WavetableSource::File(p) = source {
                        if let Ok(relative_path) = p.strip_prefix(config_dir) {
                            *source = WavetableSource::File(relative_path.to_path_buf());
                        }
                    }
                }

                let wt_preset = WavetableEnginePreset {
                    volume: state.volume.load(Ordering::Relaxed) as f32 / 1_000_000.0,
                    amp_adsr: state.amp_adsr,
                    filter_adsr: state.filter_adsr,
                    filter: *state.filter_settings.read().unwrap(),
                    lfo_settings: *state.lfo_settings.read().unwrap(),
                    lfo2_settings: *state.lfo2_settings.read().unwrap(),
                    mod_matrix: state.mod_matrix.read().unwrap().clone(),
                    saturation_settings: *state.saturation_settings.read().unwrap(),
                    wavetable_position_m_u32: state.wavetable_position.load(Ordering::Relaxed),
                    is_polyphonic: state.is_polyphonic,
                    wavetable_sources: sources,
                    window_positions: state.window_positions,
                    wavetable_mixer: *state.wavetable_mixer_settings.read().unwrap(),
                };
                SynthEnginePreset::Wavetable(wt_preset)
            }
            EngineState::Sampler(state) => {
                let mut relative_paths: [Option<PathBuf>; NUM_SAMPLE_SLOTS] = Default::default();
                for (i, path_opt) in state.sample_paths.iter().enumerate() {
                    if let Some(p) = path_opt {
                        relative_paths[i] =
                            Some(p.strip_prefix(config_dir).unwrap_or(p).to_path_buf());
                    }
                }

                let sampler_preset = sampler_engine::SamplerEnginePreset {
                    volume: state.volume.load(Ordering::Relaxed) as f32 / 1_000_000.0,
                    amp_adsr: state.amp_adsr,
                    filter_adsr: state.filter_adsr,
                    filter: *state.filter_settings.read().unwrap(),
                    lfo_settings: *state.lfo_settings.read().unwrap(),
                    lfo2_settings: *state.lfo2_settings.read().unwrap(),
                    mod_matrix: state.mod_matrix.read().unwrap().clone(),
                    saturation_settings: *state.saturation_settings.read().unwrap(),
                    is_polyphonic: state.is_polyphonic,
                    sample_paths: relative_paths,
                    root_notes: state.root_notes,
                    global_fine_tune_cents: state.global_fine_tune_cents,
                    fade_out: state.fade_out,
                };
                SynthEnginePreset::Sampler(sampler_preset)
            }
        }
    }

    pub fn load_preset(&mut self) {
        if let Some(config_dir) = settings::get_config_dir() {
            let presets_dir = config_dir.join("SynthPresets");
            if let Some(path) = FileDialog::new()
                .add_filter("json", &["json"])
                .set_directory(&presets_dir)
                .pick_file()
            {
                self.load_preset_from_path(&path);
            }
        }
    }

    pub fn save_theme(&mut self) {
        if let Some(config_dir) = settings::get_config_dir() {
            let themes_dir = config_dir.join("Themes");
            if let Some(path) = FileDialog::new()
                .add_filter("json", &["json"])
                .set_directory(&themes_dir)
                .save_file()
            {
                if let Ok(json) = serde_json::to_string_pretty(&self.theme) {
                    if let Err(e) = fs::write(&path, json) {
                        eprintln!("Failed to save theme: {}", e);
                    } else {
                        self.settings.last_theme = Some(path);
                        self.rescan_available_themes();
                    }
                }
            }
        }
    }

    pub fn load_theme(&mut self) {
        if let Some(config_dir) = settings::get_config_dir() {
            let themes_dir = config_dir.join("Themes");
            if let Some(path) = FileDialog::new()
                .add_filter("json", &["json"])
                .set_directory(&themes_dir)
                .pick_file()
            {
                self.load_theme_from_path(&path);
            }
        }
    }
    

    pub fn load_theme_from_path(&mut self, path: &Path) {
        // --- Step 1: Resolve the path if it's relative ---
        // The `path` coming from settings might be relative. We need its absolute form to read it.
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(config_dir) = settings::get_config_dir() {
            config_dir.join(path)
        } else {
            path.to_path_buf() // Fallback
        };

        // --- Step 2: Try to read the theme file from the resolved absolute path ---
        if let Ok(json_string) = fs::read_to_string(&absolute_path) {
            match serde_json::from_str::<Theme>(&json_string) {
                Ok(loaded_theme) => {
                    // --- Step 3: THE FIX ---
                    // Now, store a relative path back into settings if possible.
                    self.theme = loaded_theme;
                    if let Some(config_dir) = settings::get_config_dir() {
                        if let Ok(relative_path) = absolute_path.strip_prefix(&config_dir) {
                            // Store the nice, portable, relative path
                            self.settings.last_theme = Some(relative_path.to_path_buf());
                        } else {
                            // The theme is outside our portable folder, store absolute path as a fallback
                            self.settings.last_theme = Some(absolute_path);
                        }
                    } else {
                        // Can't get config dir, store absolute path
                        self.settings.last_theme = Some(absolute_path);
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Failed to parse theme file '{}', using default. Error: {}",
                        absolute_path.display(),
                        e
                    );
                    self.theme = Theme::default();
                }
            }
        } else {
            eprintln!(
                "Failed to read theme file '{}', using default.",
                absolute_path.display()
            );
            self.theme = Theme::default();
        }
    }

    pub fn set_engine_type(&mut self, engine_index: usize, is_wavetable: bool) {
        let needs_change = match (&self.engine_states[engine_index], is_wavetable) {
            (EngineState::Wavetable(_), false) => true,
            (EngineState::Sampler(_), true) => true,
            _ => false,
        };

        if needs_change {
            // 1. Update the UI-side state holder to a new default engine of the target type.
            if is_wavetable {
                self.engine_states[engine_index] = EngineState::new_wavetable();
                self.active_synth_section[engine_index] = SynthUISection::Wavetable;
            } else {
                self.engine_states[engine_index] = EngineState::new_sampler();
                self.active_synth_section[engine_index] = SynthUISection::Sampler;
            }

            // 2. Get the parameters for the *newly created* default state.
            let engine_params_with_vol_peak = self.get_engine_params(engine_index);

            // 3. Send a command to the audio thread to swap its engine instance.
            self.send_command(AudioCommand::ChangeEngineType {
                engine_index,
                volume: engine_params_with_vol_peak.0.clone(),
                peak_meter: engine_params_with_vol_peak.1.clone(),
                params: engine_params_with_vol_peak.2.clone(),
            });

            // 4. Send commands to initialize the new engine with default settings.
            if is_wavetable {
                self.initialize_wavetable_preset(engine_index);
            } else {
                self.initialize_sampler_preset(engine_index);
            }
        }
    }

    pub fn initialize_new_preset(&mut self) {
        self.settings.last_synth_preset = None;
        self.current_session_path = None; // A new preset means it's an unsaved session

        // We no longer stop/start audio. Instead, we send commands to reset the synth engines.
        for engine_index in 0..2 {
            // 1. Reset the UI-side state holder to a new default wavetable engine.
            self.engine_states[engine_index] = EngineState::new_wavetable();
            self.active_synth_section[engine_index] = SynthUISection::Wavetable;
            if let EngineState::Wavetable(state) = &mut self.engine_states[engine_index] {
                state.force_redraw_generation += 1;
            }

            // 2. Get the parameters for the *newly created* default state.
            // This is crucial for the ChangeEngineType command.
            let engine_params_with_vol_peak = self.get_engine_params(engine_index);

            // 3. Send a command to the audio thread to swap its engine instance with a new default one.
            // This handles both cases (was sampler or was wavetable) by simply replacing it.
            self.send_command(AudioCommand::ChangeEngineType {
                engine_index,
                volume: engine_params_with_vol_peak.0.clone(),
                peak_meter: engine_params_with_vol_peak.1.clone(),
                params: engine_params_with_vol_peak.2.clone(),
            });

            // 4. Send commands to initialize the new engine with default settings.
            // This ensures all parameters (like default wavetables) are explicitly loaded on the audio thread.
            let default_adsr = crate::synth::AdsrSettings::default();
            self.send_command(AudioCommand::SetAmpAdsr(engine_index, default_adsr));
            self.send_command(AudioCommand::SetFilterAdsr(engine_index, default_adsr));
            self.send_command(AudioCommand::ResetWavetables(engine_index));
            self.send_command(AudioCommand::SetSynthMode(engine_index, true));
        }

        // Make the new preset immediately playable
        self.send_command(AudioCommand::ActivateSynth);
        self.send_command(AudioCommand::DeactivateSampler);
    }

    pub fn initialize_wavetable_preset(&mut self, engine_index: usize) {
        // Get immutable data before the mutable borrow
        let active_sr = self.active_sample_rate;

        // Pass 1: All mutable updates to state
        if let EngineState::Wavetable(engine_state) = &mut self.engine_states[engine_index] {
            let default_adsr = crate::synth::AdsrSettings::default();
            engine_state.amp_adsr = default_adsr;
            engine_state.filter_adsr = default_adsr;
            *engine_state.filter_settings.write().unwrap() = Default::default();
            *engine_state.lfo_settings.write().unwrap() = Default::default();
            *engine_state.lfo2_settings.write().unwrap() = Default::default();
            engine_state.mod_matrix.write().unwrap().clear();
            *engine_state.wavetable_mixer_settings.write().unwrap() = Default::default();
            *engine_state.saturation_settings.write().unwrap() = Default::default();
            engine_state.wavetable_position.store(0, Ordering::Relaxed);
            engine_state.window_positions = [0.0; 4];
            engine_state.is_polyphonic = true;
            engine_state.volume.store(1_000_000, Ordering::Relaxed);

            let default_tables = wavetable_engine::WavetableSet::new_basic();
            for k in 0..4 {
                let table_data = Arc::new(default_tables.tables[k].table.clone());
                engine_state.wavetable_names[k] = default_tables.tables[k].name.clone();
                engine_state.wavetable_sources[k] =
                    WavetableSource::Default(default_tables.tables[k].name.clone());
                engine_state.original_sources[k] = table_data;
                engine_state.source_sample_rates[k] = active_sr;
            }
        }

        // Pass 2: Immutable calls to generate and send wavetables
        for k in 0..4 {
            self.generate_and_send_wavetable(engine_index, k, 0.0);
        }

        // Pass 3: Send other commands
        let default_adsr = crate::synth::AdsrSettings::default();
        self.send_command(AudioCommand::SetAmpAdsr(engine_index, default_adsr));
        self.send_command(AudioCommand::SetFilterAdsr(engine_index, default_adsr));
        self.send_command(AudioCommand::ResetWavetables(engine_index));
        self.send_command(AudioCommand::SetSynthMode(engine_index, true));
    }

    pub fn initialize_sampler_preset(&mut self, engine_index: usize) {
        // A vector to collect commands, so we don't borrow `self` mutably and immutably at once.
        let mut commands_to_send = Vec::new();

        if let EngineState::Sampler(engine_state) = &mut self.engine_states[engine_index] {
            // Reset all UI state first
            let default_adsr = crate::synth::AdsrSettings::default();
            engine_state.amp_adsr = default_adsr;
            engine_state.filter_adsr = default_adsr;
            *engine_state.filter_settings.write().unwrap() = Default::default();
            *engine_state.lfo_settings.write().unwrap() = Default::default();
            *engine_state.lfo2_settings.write().unwrap() = Default::default();
            engine_state.mod_matrix.write().unwrap().clear();
            *engine_state.saturation_settings.write().unwrap() = Default::default();
            engine_state.is_polyphonic = true;
            engine_state.volume.store(1_000_000, Ordering::Relaxed);
            engine_state.global_fine_tune_cents = 0.0;
            engine_state.fade_out = 0.01;
            engine_state.root_notes = std::array::from_fn(|i| (24 + i * 12) as u8); // C2, C3...

            // Collect commands for global settings first
            commands_to_send.push(AudioCommand::SetAmpAdsr(engine_index, default_adsr));
            commands_to_send.push(AudioCommand::SetFilterAdsr(engine_index, default_adsr));
            commands_to_send.push(AudioCommand::SetSamplerSettings {
                engine_index,
                root_notes: engine_state.root_notes,
                global_fine_tune_cents: engine_state.global_fine_tune_cents,
                fade_out: engine_state.fade_out,
            });
            commands_to_send.push(AudioCommand::SetSynthMode(engine_index, true));

            // Clear all sample slots on UI and collect commands for the audio thread
            for i in 0..NUM_SAMPLE_SLOTS {
                engine_state.sample_names[i] = "Empty".to_string();
                engine_state.sample_paths[i] = None;
                engine_state.sample_data_for_ui[i].write().unwrap().clear();
                // Command to clear sample on audio thread
                commands_to_send.push(AudioCommand::LoadSampleForSamplerSlot {
                    engine_index,
                    slot_index: i,
                    audio_data: Arc::new(vec![]),
                });
            }
        } // The mutable borrow of `engine_state` (and thus `self`) ends here.

        // Now it's safe to call `send_command` on `self`.
        for cmd in commands_to_send {
            self.send_command(cmd);
        }
    }

    /// This function lives on the UI thread and performs the heavy lifting.
    pub fn generate_and_send_wavetable(
        &self,
        engine_index: usize,
        slot_index: usize,
        window_pos: f32,
    ) {
        if let EngineState::Wavetable(wt_state) = &self.engine_states[engine_index] {
            let source_data = wt_state.original_sources[slot_index].clone();
            let source_sr = wt_state.source_sample_rates[slot_index] as f32;
            let target_sr = self.active_sample_rate as f32;
            let name = wt_state.wavetable_names[slot_index].clone();
            let mut new_table = vec![0.0; WAVETABLE_SIZE];

            if !source_data.is_empty() {
                let ratio = target_sr as f64 / source_sr as f64;
                let input_len = (WAVETABLE_SIZE as f64 / ratio).ceil() as usize;

                if (source_sr - target_sr).abs() < 1e-3 || input_len == 0 {
                    // No resampling needed or invalid input length
                    let slice_len = WAVETABLE_SIZE.min(source_data.len());
                    let max_start_index = source_data.len().saturating_sub(slice_len);
                    let start_index = (window_pos * max_start_index as f32).round() as usize;
                    let end_index = start_index + slice_len;
                    let slice = &source_data[start_index..end_index];
                    new_table[..slice.len()].copy_from_slice(slice);
                } else {
                    // Resampling is needed
                    let input_len_clamped = input_len.min(source_data.len());
                    let max_start_index = source_data.len().saturating_sub(input_len_clamped);
                    let start_index = (window_pos * max_start_index as f32).round() as usize;
                    let end_index = start_index + input_len_clamped;
                    let slice = &source_data[start_index..end_index];

                    let params = SincInterpolationParameters {
                        sinc_len: 256,
                        f_cutoff: 0.95,
                        interpolation: SincInterpolationType::Linear,
                        oversampling_factor: 256,
                        window: WindowFunction::BlackmanHarris2,
                    };
                    if let Ok(mut resampler) =
                        SincFixedIn::<f32>::new(ratio, 2.0, params, slice.len(), 1)
                    {
                        let waves_in = vec![slice.to_vec()];
                        if let Ok(waves_out) = resampler.process(&waves_in, None) {
                            if let Some(resampled_data) = waves_out.into_iter().next() {
                                let len_to_copy = resampled_data.len().min(WAVETABLE_SIZE);
                                new_table[..len_to_copy]
                                    .copy_from_slice(&resampled_data[..len_to_copy]);
                            }
                        }
                    }
                }

                // Normalize the final wavetable
                let max_abs = new_table
                    .iter()
                    .fold(0.0f32, |max, &val| max.max(val.abs()));
                if max_abs > 1e-6 {
                    let inv_max = 1.0 / max_abs;
                    for sample in &mut new_table {
                        *sample *= inv_max;
                    }
                }
            }
            self.send_command(AudioCommand::SetWavetable {
                engine_index,
                slot_index,
                audio_data: Arc::new(new_table),
                name,
            });
        }
    }

    /// Update the theory view based on the currently held MIDI notes.
    fn update_theory_display(&mut self) {
        let notes = self.live_midi_notes.read().unwrap().clone();

        match self.theory_mode {
            TheoryMode::Scales => {
                // In scale mode, we only update the display if exactly one note is held.
                // Otherwise, the previously displayed scale remains ("sticky").
                if notes.len() == 1 {
                    if let Some(&root_note) = notes.iter().next() {
                        // Clear the display ONLY when we are about to draw a new valid scale.
                        self.displayed_theory_notes.clear();
                        let scale_notes = theory::get_scale_notes(root_note, self.selected_scale);
                        for (i,&note) in scale_notes.iter().enumerate() {
                            self.displayed_theory_notes.push((note, i % NUM_LOOPERS));
                        }
                    }
                }
            }
            TheoryMode::Chords => {
                // If the held notes are the same as the ones that triggered the last suggestion, do nothing.
                if notes == self.last_recognized_chord_notes {
                    return;
                }

                // If the current notes are a subset of the last chord (i.e., user is releasing keys),
                // don't update the display. This makes it "sticky".
                // We check !notes.is_empty() to ensure the latch still clears on the final key release.
                if !notes.is_empty() && notes.is_subset(&self.last_recognized_chord_notes) {
                    return;
                }

                // If no keys are pressed, clear the last recognized chord to allow re-triggering.
                // Do not clear the display, making it sticky.
                if notes.is_empty() {
                    self.last_recognized_chord_notes.clear();
                    return;
                }

                if notes.len() >= 2 {
                    if let Some(chord) = theory::recognize_chord(&notes) {
                        // A new, valid chord has been recognized. Update the display.
                        self.last_recognized_chord_notes = notes.clone(); // Latch the new chord
                        self.displayed_theory_notes.clear(); // Clear old suggestions

                        let suggestions =
                            theory::get_chord_suggestions(&chord, &self.selected_chord_style);

                        match self.chord_display_mode {
                            ChordDisplayMode::Spread => {
                                // Use fixed, non-adjacent octaves 1, 3, 5, and 7.
                                let display_octaves = [1, 3, 5, 7];

                                for (i, (quality, root)) in suggestions.iter().enumerate() {
                                    if let Some(&octave_to_use) = display_octaves.get(i) {
                                        let chord_notes =
                                            theory::build_chord_notes(*root, *quality, octave_to_use);

                                        for note in chord_notes {
                                            if note <= 127 {
                                                self.displayed_theory_notes.push((note, i));
                                            }
                                        }
                                    }
                                }
                            }
                            ChordDisplayMode::Stacked => {
                                // For stacked mode, we place all chords in a central octave (e.g. 4)
                                const STACK_OCTAVE: u8 = 4;
                                for (i, (quality, root)) in suggestions.iter().enumerate() {
                                    let chord_notes =
                                        theory::build_chord_notes(*root, *quality, STACK_OCTAVE);
                                    for note in chord_notes {
                                        if note <= 127 {
                                            self.displayed_theory_notes.push((note, i));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // If chord is not recognized (e.g. just two notes), do nothing. The display remains sticky.
                }
            }
        }
    }

    pub fn save_session(&mut self, path_override: Option<PathBuf>) {
        let session_path = match path_override {
            Some(p) => p,
            None => {
                if let Some(config_dir) = settings::get_config_dir() {
                    let sessions_dir = config_dir.join("Sessions");
                    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
                    let filename = format!("Session_{}", timestamp);
                    if let Some(path) = FileDialog::new()
                        .set_directory(&sessions_dir)
                        .set_file_name(&filename)
                        .save_file()
                    {
                        path
                    } else {
                        return; // User cancelled dialog
                    }
                } else {
                    return;
                }
            }
        };

        let session_dir = session_path.with_extension("");

        if let Err(e) = fs::create_dir_all(&session_dir) {
            eprintln!("Failed to create session directory: {}", e);
            return;
        }

        if let Some(config_dir) = settings::get_config_dir() {
            let mixer_state = MixerState {
                tracks: self.track_mixer_state.read().unwrap().tracks,
                master_volume_m_u32: self.master_volume.load(Ordering::Relaxed),
                limiter_is_active: self.limiter_is_active.load(Ordering::Relaxed),
                limiter_threshold_m_u32: self.limiter_threshold.load(Ordering::Relaxed),
                limiter_release_mode: self.limiter_release_mode,
                limiter_release_ms_m_u32: self.limiter_release_ms.load(Ordering::Relaxed),
                limiter_release_sync_rate_m_u32: self.limiter_release_sync_rate.load(Ordering::Relaxed),
            };

            let synth_preset_path = self.settings.last_synth_preset.as_ref().and_then(|p| {
                p.strip_prefix(&config_dir).ok().map(|rp| rp.to_path_buf())
            });
            let sampler_kit_path = self.settings.last_sampler_kit.as_ref().and_then(|p| {
                p.strip_prefix(&config_dir).ok().map(|rp| rp.to_path_buf())
            });

            let session_data = SessionData {
                mixer_state,
                synth_preset_path,
                sampler_kit_path,
                is_input_armed: self.audio_input_is_armed.load(Ordering::Relaxed),
                is_input_monitored: self.audio_input_is_monitored.load(Ordering::Relaxed),
                transport_len_samples: self.transport_len_samples.load(Ordering::Relaxed),
                original_sample_rate: self.active_sample_rate,
            };

            let json_path = session_dir.join("session.json");
            if let Ok(json_string) = serde_json::to_string_pretty(&session_data) {
                if let Err(e) = fs::write(&json_path, json_string) {
                    eprintln!("Failed to write session.json: {}", e);
                    return;
                }
            } else {
                eprintln!("Failed to serialize session data.");
                return;
            }

            self.send_command(AudioCommand::SaveSessionAudio { session_path: session_dir.clone() });
            self.current_session_path = Some(session_dir);
            self.rescan_asset_library();
        }
    }

    pub fn load_session(&mut self, path: &Path) {
        let json_path = path.join("session.json");
        let json_string = match fs::read_to_string(&json_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to read session file {}: {}", json_path.display(), e);
                return;
            }
        };

        let session_data: SessionData = match serde_json::from_str(&json_string) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to parse session file {}: {}", json_path.display(), e);
                return;
            }
        };

        // --- Begin state restoration ---
        self.send_command(AudioCommand::ClearAll);

        // Send the entire mixer state to the audio thread for atomic update
        let mixer_state = session_data.mixer_state.clone();
        self.send_command(AudioCommand::SetMixerState(mixer_state));

        // Also update the UI's direct view of the state
        *self.track_mixer_state.write().unwrap() = session_data.mixer_state.clone();
        self.master_volume.store(session_data.mixer_state.master_volume_m_u32, Ordering::Relaxed);
        self.limiter_is_active.store(session_data.mixer_state.limiter_is_active, Ordering::Relaxed);
        self.limiter_threshold.store(session_data.mixer_state.limiter_threshold_m_u32, Ordering::Relaxed);
        self.limiter_release_mode = session_data.mixer_state.limiter_release_mode; // This was the missing line
        self.limiter_release_ms.store(session_data.mixer_state.limiter_release_ms_m_u32, Ordering::Relaxed);
        self.limiter_release_sync_rate.store(session_data.mixer_state.limiter_release_sync_rate_m_u32, Ordering::Relaxed);

        if let Some(relative_path) = session_data.synth_preset_path {
            if let Some(config_dir) = settings::get_config_dir() {
                let full_path = config_dir.join(relative_path);
                self.load_preset_from_path(&full_path);
            }
        }
        if let Some(relative_path) = session_data.sampler_kit_path {
            if let Some(config_dir) = settings::get_config_dir() {
                let full_path = config_dir.join(relative_path);
                self.load_kit(&full_path);
            }
        }

        self.audio_input_is_armed.store(session_data.is_input_armed, Ordering::Relaxed);
        self.audio_input_is_monitored.store(session_data.is_input_monitored, Ordering::Relaxed);

        for i in 0..NUM_LOOPERS {
            let loop_filename = format!("loop_{}.wav", i);
            let loop_path = path.join(loop_filename);
            if loop_path.exists() {
                self.send_command(AudioCommand::LoadLoopAudio {
                    looper_index: i,
                    path: loop_path,
                    original_sample_rate: session_data.original_sample_rate,
                });
            }
        }
        self.send_command(AudioCommand::SetTransportLen(session_data.transport_len_samples));
        self.current_session_path = Some(path.to_path_buf());
    }
}

impl eframe::App for CypherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- State Updates ---
        // Must be called before any UI is drawn to ensure the display is up to date.
        self.update_theory_display();

        if self.should_toggle_record_from_midi.swap(false, Ordering::Relaxed) {
            self.is_recording_output = !self.is_recording_output;
            if self.is_recording_output {
                self.send_command(AudioCommand::StartOutputRecording);
            } else {
                if let Some(config_dir) = settings::get_config_dir() {
                    let rec_dir = config_dir.join("LiveRecordings");
                    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
                    let filename = format!("LiveRec_{}.wav", timestamp);
                    let path = rec_dir.join(filename);
                    self.send_command(AudioCommand::StopOutputRecording { output_path: path.clone() });
                    self.recording_notification = Some((format!("Saved to {}", path.display()), Instant::now()));
                }
            }
        }

        if let Some((_, time)) = self.recording_notification {
            if time.elapsed() > std::time::Duration::from_secs(5) {
                self.recording_notification = None;
            }
        }

        if let Ok(mut last_msg) = self.last_midi_cc_message.write() {
            if let Some((_, time)) = *last_msg {
                if time.elapsed() > std::time::Duration::from_secs(3) {
                    *last_msg = None;
                }
            }
        }

        // --- DEADLOCK-SAFE MIDI LEARN LOGIC ---
        // 1. Read the target and immediately release the read lock.
        let learn_target = *self.midi_mod_matrix_learn_target.read().unwrap();

        // 2. If a target is set, proceed.
        if let Some((engine_index, slot_index)) = learn_target {
            // 3. Try to take a value from the `last_learned` source.
            if let Some(control_id) = self.last_learned_mod_source.write().unwrap().take() {
                // 4. A value was learned! Update the UI state.
                match &mut self.engine_states[engine_index] {
                    EngineState::Wavetable(state) => {
                        if let Some(routing) = state.mod_matrix.write().unwrap().get_mut(slot_index) {
                            routing.source = ModSource::MidiCC(control_id);
                        }
                    }
                    EngineState::Sampler(state) => {
                        if let Some(routing) = state.mod_matrix.write().unwrap().get_mut(slot_index) {
                            routing.source = ModSource::MidiCC(control_id);
                        }
                    }
                }
                // 5. Clear the learn target, ending the learn mode.
                *self.midi_mod_matrix_learn_target.write().unwrap() = None;
            }
        }

        let visuals: egui::Visuals = (&self.theme).into();
        ctx.set_visuals(visuals);

        ctx.request_repaint_after(std::time::Duration::from_millis(10));

        // --- Peak Meter Decay Logic ---
        for i in 0..NUM_LOOPERS {
            let new_peak = self.peak_meters[i].load(Ordering::Relaxed) as f32 / u32::MAX as f32;
            self.displayed_peak_levels[i] = (self.displayed_peak_levels[i] * 0.95).max(new_peak);
        }
        let new_input_peak = self.input_peak_meter.load(Ordering::Relaxed) as f32 / u32::MAX as f32;
        self.displayed_input_peak_level =
            (self.displayed_input_peak_level * 0.95).max(new_input_peak);

        for i in 0..2 {
            match &mut self.engine_states[i] {
                EngineState::Wavetable(state) => {
                    let new_peak =
                        state.peak_meter.load(Ordering::Relaxed) as f32 / u32::MAX as f32;
                    state.displayed_peak_level = (state.displayed_peak_level * 0.95).max(new_peak);
                }
                EngineState::Sampler(state) => {
                    let new_peak =
                        state.peak_meter.load(Ordering::Relaxed) as f32 / u32::MAX as f32;
                    state.displayed_peak_level = (state.displayed_peak_level * 0.95).max(new_peak);
                }
            }
        }

        let new_synth_master_peak =
            self.synth_master_peak_meter.load(Ordering::Relaxed) as f32 / u32::MAX as f32;
        self.displayed_synth_master_peak_level =
            (self.displayed_synth_master_peak_level * 0.95).max(new_synth_master_peak);

        let new_sampler_peak =
            self.sampler_peak_meter.load(Ordering::Relaxed) as f32 / u32::MAX as f32;
        self.displayed_sampler_peak_level =
            (self.displayed_sampler_peak_level * 0.95).max(new_sampler_peak);

        let new_master_peak =
            self.master_peak_meter.load(Ordering::Relaxed) as f32 / u32::MAX as f32;
        self.displayed_master_peak_level =
            (self.displayed_master_peak_level * 0.95).max(new_master_peak);

        let new_gr = self.gain_reduction_db.load(Ordering::Relaxed) as f32 / 24_000_000.0;
        self.displayed_gain_reduction = (self.displayed_gain_reduction * 0.92).max(new_gr);

        // --- UI Drawing ---
        ui::draw_main_view(self, ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop_audio();
        self.save_settings();
    }
}