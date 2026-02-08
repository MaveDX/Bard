use std::path::Path;
use std::process::Command;

/// Normalized stereo peak pair (0.0–1.0)
#[derive(Clone, Debug)]
pub struct PeakPair {
    pub left: f64,
    pub right: f64,
}

/// Holds the waveform peaks for a song.
#[derive(Clone, Debug)]
pub struct WaveformData {
    pub peaks: Vec<PeakPair>,
}

impl WaveformData {
    /// Extract waveform peaks from an audio file using ffmpeg.
    /// Returns `num_bars` peaks, each normalized 0.0–1.0.
    /// This is CPU-intensive and should be called from a background thread.
    pub fn from_file(path: &str, num_bars: usize) -> Option<Self> {
        if !Path::new(path).exists() {
            return None;
        }

        // Use ffmpeg to decode audio to raw signed 16-bit stereo PCM at 8kHz
        // (low sample rate = fast extraction, still enough resolution for waveform)
        let output = Command::new("ffmpeg")
            .args(&[
                "-i", path,
                "-ac", "2",          // stereo
                "-ar", "8000",       // 8kHz sample rate
                "-f", "s16le",       // raw signed 16-bit little-endian
                "-acodec", "pcm_s16le",
                "-v", "quiet",
                "-",                 // output to stdout
            ])
            .output()
            .ok()?;

        if !output.status.success() || output.stdout.is_empty() {
            return None;
        }

        let raw = &output.stdout;
        // Each sample frame = 4 bytes (2 bytes left + 2 bytes right, s16le)
        let num_frames = raw.len() / 4;
        if num_frames == 0 || num_bars == 0 {
            return None;
        }

        let frames_per_bar = (num_frames as f64 / num_bars as f64).max(1.0);
        let mut peaks = Vec::with_capacity(num_bars);

        let mut frame_idx: f64 = 0.0;
        for _ in 0..num_bars {
            let start = frame_idx as usize;
            let end = ((frame_idx + frames_per_bar) as usize).min(num_frames);

            let mut sum_left: f64 = 0.0;
            let mut sum_right: f64 = 0.0;
            let mut count: f64 = 0.0;

            for f in start..end {
                let offset = f * 4;
                if offset + 3 >= raw.len() {
                    break;
                }
                let left = i16::from_le_bytes([raw[offset], raw[offset + 1]]);
                let right = i16::from_le_bytes([raw[offset + 2], raw[offset + 3]]);
                let l = left.unsigned_abs() as f64;
                let r = right.unsigned_abs() as f64;
                sum_left += l * l;
                sum_right += r * r;
                count += 1.0;
            }

            // RMS (root mean square) gives a more musical representation
            let rms_left = if count > 0.0 { (sum_left / count).sqrt() } else { 0.0 };
            let rms_right = if count > 0.0 { (sum_right / count).sqrt() } else { 0.0 };

            peaks.push(PeakPair {
                left: rms_left,
                right: rms_right,
            });

            frame_idx += frames_per_bar;
        }

        // Normalize using the 95th percentile so only the loudest bars peak,
        // then apply a power curve to spread out the dynamic range.
        let mut all_vals: Vec<f64> = peaks.iter()
            .flat_map(|p| [p.left, p.right])
            .filter(|v| *v > 0.0)
            .collect();
        all_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let norm_val = if !all_vals.is_empty() {
            let idx = ((all_vals.len() as f64) * 0.95) as usize;
            let idx = idx.min(all_vals.len() - 1);
            all_vals[idx]
        } else {
            1.0
        };

        if norm_val > 0.0 {
            for p in peaks.iter_mut() {
                // Normalize against 95th percentile (top 5% clips to 1.0)
                p.left = (p.left / norm_val).min(1.0);
                p.right = (p.right / norm_val).min(1.0);
                // Cube-root power curve: stretches lows, compresses highs
                p.left = p.left.powf(1.8);
                p.right = p.right.powf(1.8);
            }
        }

        Some(WaveformData { peaks })
    }

    /// Calculate the number of bars that fit in a given pixel width.
    /// Each bar is `bar_width` px wide with `gap` px spacing.
    pub fn bars_for_width(width: i32, bar_width: i32, gap: i32) -> usize {
        let block = bar_width + gap;
        if block <= 0 { return 0; }
        (width / block) as usize
    }
}

/// Draw the waveform onto a Cairo context.
/// - `peaks`: the peak data
/// - `position`: 0.0–1.0 playback position
/// - `w`, `h`: widget dimensions
/// - `played_color`: (r, g, b, a) for played bars
/// - `unplayed_color`: (r, g, b, a) for unplayed bars
pub fn draw_waveform(
    cr: &cairo::Context,
    peaks: &[PeakPair],
    position: f64,
    w: f64,
    h: f64,
) {
    if peaks.is_empty() || w <= 0.0 || h <= 0.0 {
        return;
    }

    let bar_width: f64 = 2.0;
    let gap: f64 = 2.0;
    let block = bar_width + gap;
    let center_y = h / 2.0;

    let n_bars = peaks.len();
    let waveform_width = n_bars as f64 * block;
    let offset_x = (w - waveform_width).max(0.0) / 2.0;
    let cursor_x = position.clamp(0.0, 1.0) * waveform_width;

    // Minimum bar height so empty bars are still visible
    let min_bar_h = 2.0;

    for (i, peak) in peaks.iter().enumerate() {
        let x = offset_x + i as f64 * block;
        let bar_x = x - offset_x; // position within waveform

        // Left peak goes up from center, right goes down
        let left_h = (peak.left * (h / 2.0 - 1.0)).max(min_bar_h / 2.0);
        let right_h = (peak.right * (h / 2.0 - 1.0)).max(min_bar_h / 2.0);
        let total_h = left_h + right_h;
        let y = center_y - left_h;

        if bar_x < cursor_x {
            // Played: bright white
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
        } else {
            // Unplayed: dim white
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.25);
        }

        // Draw rounded-ish bar (just a rect at 2px wide)
        cr.rectangle(x, y, bar_width, total_h);
        cr.fill().unwrap();
    }
}

/// Draw a placeholder waveform (no peaks loaded yet) as small dots
pub fn draw_placeholder(
    cr: &cairo::Context,
    w: f64,
    h: f64,
) {
    let bar_width: f64 = 2.0;
    let gap: f64 = 2.0;
    let block = bar_width + gap;
    let center_y = h / 2.0;

    let mut x = gap;
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.3);
    while x < w - gap {
        cr.rectangle(x, center_y - 1.0, bar_width, 2.0);
        cr.fill().unwrap();
        x += block;
    }
}
