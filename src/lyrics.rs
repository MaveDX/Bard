use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct LyricLine {
    pub timestamp: f64,
    pub text: String,
}

pub struct LRCParser {
    pub lines: Vec<LyricLine>,
}

impl LRCParser {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Option<Self> {
        let file = File::open(path).ok()?;
        let reader = BufReader::new(file);
        
        let mut lines = Vec::new();
        let time_regex = regex::Regex::new(r"\[(\d+):(\d+\.\d+)\](.*)").ok()?;

        for line in reader.lines() {
            let line = line.ok()?;
            
            if let Some(captures) = time_regex.captures(&line) {
                let minutes: u32 = captures.get(1)?.as_str().parse().ok()?;
                let seconds: f64 = captures.get(2)?.as_str().parse().ok()?;
                let text = captures.get(3)?.as_str().trim().to_string();
                
                let timestamp = minutes as f64 * 60.0 + seconds;
                
                lines.push(LyricLine { timestamp, text });
            }
        }

        lines.sort_by(|a, b| a.timestamp.partial_cmp(&b.timestamp).unwrap());

        Some(Self { lines })
    }

    pub fn get_current_line(&self, current_time: f64) -> Option<(usize, &str)> {
        for (i, line) in self.lines.iter().enumerate() {
            if i + 1 < self.lines.len() {
                let next_timestamp = self.lines[i + 1].timestamp;
                if line.timestamp <= current_time && current_time < next_timestamp {
                    return Some((i, &line.text));
                }
            } else if line.timestamp <= current_time {
                return Some((i, &line.text));
            }
        }
        None
    }
}
