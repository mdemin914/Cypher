use serde::{Deserialize, Serialize};
use std::path::PathBuf;


#[derive(Serialize, Deserialize, Default, Debug)]
pub struct SamplerKit {
    // An array of 16 optional paths. `None` means the pad is empty.
    pub pads: [Option<PathBuf>; 16],
}