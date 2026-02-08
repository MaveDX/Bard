use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

/// Manages a CAVA audio visualizer subprocess that outputs raw bar data.
/// Reads the user's config from ~/.config/cava/config and overrides
/// the output section to use raw binary mode for internal rendering.
pub struct CavaVisualizer {
    process: Option<Child>,
    bars: Arc<Mutex<Vec<u8>>>,
    num_bars: usize,
    temp_config_path: String,
}

impl CavaVisualizer {
    /// Spawn a new CAVA process with `num_bars` bars.
    /// Returns None if cava is not installed or fails to start.
    pub fn new(num_bars: usize) -> Option<Self> {
        let temp_config_path = Self::create_temp_config(num_bars)?;

        let mut child = Command::new("cava")
            .arg("-p")
            .arg(&temp_config_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .ok()?;

        let bars = Arc::new(Mutex::new(vec![0u8; num_bars]));
        let bars_clone = bars.clone();
        let bar_count = num_bars;

        if let Some(stdout) = child.stdout.take() {
            thread::spawn(move || {
                let mut reader = stdout;
                let mut buf = vec![0u8; bar_count];
                loop {
                    match reader.read_exact(&mut buf) {
                        Ok(()) => {
                            if let Ok(mut bars) = bars_clone.lock() {
                                bars.copy_from_slice(&buf);
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        Some(Self {
            process: Some(child),
            bars,
            num_bars,
            temp_config_path,
        })
    }

    /// Get the current bar values (0–255 each).
    pub fn get_bars(&self) -> Vec<u8> {
        self.bars.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Get a clone of the Arc holding bar data, for sharing with draw callbacks.
    pub fn get_bars_arc(&self) -> Arc<Mutex<Vec<u8>>> {
        Arc::clone(&self.bars)
    }

    /// Number of bars this visualizer was created with.
    pub fn num_bars(&self) -> usize {
        self.num_bars
    }

    /// Build a temporary config that imports the user's settings but forces raw output.
    fn create_temp_config(num_bars: usize) -> Option<String> {
        let home = std::env::var("HOME").ok()?;
        let user_config_path = PathBuf::from(&home).join(".config/cava/config");

        let user_config = if user_config_path.exists() {
            std::fs::read_to_string(&user_config_path).unwrap_or_default()
        } else {
            String::new()
        };

        let temp_path = format!("/tmp/bard_cava_{}.conf", std::process::id());

        // Copy user config but strip [output] and [general] bars setting
        // so we can override them
        let mut config = String::new();
        let mut skip_section = false;

        for line in user_config.lines() {
            let trimmed = line.trim();

            // Detect section headers
            if trimmed.starts_with('[') {
                skip_section = trimmed == "[output]";
                if !skip_section {
                    config.push_str(line);
                    config.push('\n');
                }
                continue;
            }

            if skip_section {
                continue;
            }

            // Skip user's `bars =` in [general] — we'll set our own
            if trimmed.starts_with("bars") && trimmed.contains('=') {
                continue;
            }

            config.push_str(line);
            config.push('\n');
        }

        // Append our overrides
        config.push_str(&format!(
            "\n\
            [general]\n\
            bars = {}\n\
            \n\
            [output]\n\
            method = raw\n\
            raw_target = /dev/stdout\n\
            data_format = binary\n\
            bit_format = 8bit\n\
            channels = mono\n",
            num_bars
        ));

        std::fs::write(&temp_path, &config).ok()?;
        Some(temp_path)
    }
}

impl Drop for CavaVisualizer {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }
        let _ = std::fs::remove_file(&self.temp_config_path);
    }
}
