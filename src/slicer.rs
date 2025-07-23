//! Contains the core logic for detecting audible chunks based on visual peak data.

/// Finds contiguous blocks of audio based on a simplified array of peak values.
/// This function is designed to operate on the same data the user sees in the waveform view.
///
/// # Arguments
/// * `visual_peaks` - A slice where each float represents the peak amplitude for a horizontal pixel.
/// * `samples_per_pixel` - How many audio samples are represented by a single pixel/value in `visual_peaks`.
/// * `silence_threshold` - The amplitude level below which a peak is considered silent.
/// * `min_silence_ms` - The minimum duration of silence required to create a gap between slices.
/// * `sample_rate` - The sample rate of the original audio.
/// * `audio_data` - The full, original audio data for the refinement step.
///
/// # Returns
/// A `Vec<(usize, usize)>` where each tuple represents the start and end sample index
/// of a detected slice.
pub fn find_slices_from_visual_peaks(
    visual_peaks: &[f32],
    samples_per_pixel: f32,
    silence_threshold: f32,
    min_silence_ms: f32,
    sample_rate: u32,
    audio_data: &[f32],
) -> Vec<(usize, usize)> {
    if visual_peaks.is_empty() {
        return vec![];
    }

    let min_silence_pixels = (min_silence_ms / 3000.0 * sample_rate as f32 / samples_per_pixel).ceil() as usize;
    if min_silence_pixels == 0 {
        let end_sample = (visual_peaks.len() as f32 * samples_per_pixel) as usize;
        return vec![(0, end_sample.min(audio_data.len()))];
    }

    let mut rough_regions = Vec::new();
    let mut last_slice_end_pixel = 0;
    let mut consecutive_silent_pixels = 0;

    for (i, &peak) in visual_peaks.iter().enumerate() {
        if peak < silence_threshold {
            consecutive_silent_pixels += 1;
        } else {
            consecutive_silent_pixels = 0;
        }

        if consecutive_silent_pixels >= min_silence_pixels {
            let gap_start_pixel = i.saturating_sub(min_silence_pixels - 1);
            if let Some(slice_start_pixel_offset) = visual_peaks[last_slice_end_pixel..gap_start_pixel]
                .iter()
                .position(|&p| p >= silence_threshold)
            {
                let slice_start_pixel = last_slice_end_pixel + slice_start_pixel_offset;
                let slice_end_pixel = gap_start_pixel;

                let start_sample = (slice_start_pixel as f32 * samples_per_pixel) as usize;
                let end_sample = (slice_end_pixel as f32 * samples_per_pixel) as usize;

                if end_sample > start_sample {
                    rough_regions.push((start_sample, end_sample));
                }
                last_slice_end_pixel = i + 1;
            }
            consecutive_silent_pixels = 0;
        }
    }

    if last_slice_end_pixel < visual_peaks.len() {
        if let Some(slice_start_pixel_offset) = visual_peaks[last_slice_end_pixel..]
            .iter()
            .position(|&p| p >= silence_threshold)
        {
            let slice_start_pixel = last_slice_end_pixel + slice_start_pixel_offset;
            let start_sample = (slice_start_pixel as f32 * samples_per_pixel) as usize;
            let end_sample = (visual_peaks.len() as f32 * samples_per_pixel) as usize;

            if end_sample > start_sample {
                rough_regions.push((start_sample, end_sample));
            }
        }
    }

    // --- Stage 3: Refine Start Points ---
    let mut refined_regions = Vec::with_capacity(rough_regions.len());
    for (start_sample, end_sample) in rough_regions {
        // Search within the rough region for the first sample that exceeds the threshold.
        let search_area = &audio_data[start_sample.min(audio_data.len())..end_sample.min(audio_data.len())];

        let precise_start_offset = search_area
            .iter()
            .position(|&s| s.abs() >= silence_threshold)
            .unwrap_or(0); // Default to the start of the rough region if no sample is loud enough

        let precise_start = start_sample + precise_start_offset;

        if end_sample > precise_start {
            refined_regions.push((precise_start, end_sample));
        }
    }

    refined_regions
}