use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

// A simple atomic counter to generate unique IDs for widgets.
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn new_id() -> egui::Id {
    egui::Id::new(NEXT_ID.fetch_add(1, AtomicOrdering::Relaxed))
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum Asset {
    Sample(SampleRef),
    SynthPreset(SynthPresetRef),
    SamplerKit(SamplerKitRef),
}

impl Default for Asset {
    // A default asset has to be something concrete. A default SampleRef is a good candidate.
    fn default() -> Self {
        Asset::Sample(SampleRef::default())
    }
}

// Common trait for assets to get their name and path
pub trait AssetRef {
    fn name(&self) -> &str;
    fn path(&self) -> &PathBuf;
}

impl AssetRef for Asset {
    fn name(&self) -> &str {
        match self {
            Asset::Sample(r) => &r.name,
            Asset::SynthPreset(r) => &r.name,
            Asset::SamplerKit(r) => &r.name,
        }
    }
    fn path(&self) -> &PathBuf {
        match self {
            Asset::Sample(r) => &r.path,
            Asset::SynthPreset(r) => &r.path,
            Asset::SamplerKit(r) => &r.path,
        }
    }
}

// Manual implementation of Ord and PartialOrd to ignore the non-ordered `id` field.
impl PartialOrd for Asset {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Asset {
    fn cmp(&self, other: &Self) -> Ordering {
        self.path().cmp(other.path()).then_with(|| self.name().cmp(other.name()))
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SampleRef {
    pub id: egui::Id,
    pub name: String,
    pub path: PathBuf,
}

impl Default for SampleRef {
    fn default() -> Self {
        Self {
            id: new_id(),
            name: "".to_string(),
            path: Default::default(),
        }
    }
}

impl AssetRef for SampleRef {
    fn name(&self) -> &str { &self.name }
    fn path(&self) -> &PathBuf { &self.path }
}

impl SampleRef {
    pub fn new(path: PathBuf) -> Option<Self> {
        let name = path.file_stem()?.to_string_lossy().to_string();
        Some(Self {
            id: new_id(),
            name,
            path,
        })
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SynthPresetRef {
    pub id: egui::Id,
    pub name: String,
    pub path: PathBuf,
}

impl AssetRef for SynthPresetRef {
    fn name(&self) -> &str { &self.name }
    fn path(&self) -> &PathBuf { &self.path }
}

impl SynthPresetRef {
    pub fn new(path: PathBuf) -> Option<Self> {
        let name = path.file_stem()?.to_string_lossy().to_string();
        Some(Self { id: new_id(), name, path })
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SamplerKitRef {
    pub id: egui::Id,
    pub name: String,
    pub path: PathBuf,
}

impl AssetRef for SamplerKitRef {
    fn name(&self) -> &str { &self.name }
    fn path(&self) -> &PathBuf { &self.path }
}

impl SamplerKitRef {
    pub fn new(path: PathBuf) -> Option<Self> {
        let name = path.file_stem()?.to_string_lossy().to_string();
        Some(Self { id: new_id(), name, path })
    }
}


#[derive(Default, Debug, Clone)]
pub struct LibraryFolder {
    pub assets: BTreeSet<Asset>,
    pub subfolders: BTreeMap<String, LibraryFolder>,
}

impl LibraryFolder {
    pub fn insert_asset(&mut self, path_segments: &[String], asset: Asset) {
        if path_segments.is_empty() {
            return;
        }

        let folder_path = &path_segments[..path_segments.len() - 1];
        let mut current_folder = self;

        for segment in folder_path {
            current_folder = current_folder
                .subfolders
                .entry(segment.clone())
                .or_default();
        }

        current_folder.assets.insert(asset);
    }

    pub fn clear(&mut self) {
        self.assets.clear();
        self.subfolders.clear();
    }
}


#[derive(Default, Debug)]
pub struct AssetLibrary {
    pub sample_root: LibraryFolder,
    pub synth_root: LibraryFolder,
    pub kit_root: LibraryFolder,
}

impl AssetLibrary {
    pub fn clear(&mut self) {
        self.sample_root.clear();
        self.synth_root.clear();
        self.kit_root.clear();
    }
}