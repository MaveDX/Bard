use image::{DynamicImage, GenericImageView};
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct RGB {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl RGB {
    pub fn new(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b }
    }

    pub fn to_css(&self) -> String {
        format!(
            "rgb({}, {}, {})",
            (self.r * 255.0) as u8,
            (self.g * 255.0) as u8,
            (self.b * 255.0) as u8
        )
    }

    pub fn darken(&self, factor: f64) -> Self {
        Self {
            r: self.r * factor,
            g: self.g * factor,
            b: self.b * factor,
        }
    }

    pub fn desaturate(&self, amount: f64) -> Self {
        let (h, s, v) = rgb_to_hsv(self.r, self.g, self.b);
        let new_s = s * (1.0 - amount);
        let (r, g, b) = hsv_to_rgb(h, new_s, v);
        Self { r, g, b }
    }
}

pub struct ColorExtractor;

impl ColorExtractor {
    pub fn extract_from_file<P: AsRef<Path>>(path: P) -> Option<RGB> {
        let img = image::open(path).ok()?;
        Some(Self::extract_dominant_color(&img))
    }

    /// Extract a palette of 4 distinct colors from the image for gradient backgrounds.
    /// Returns (top-left, top-right, bottom-left, bottom-right) colors.
    pub fn extract_palette<P: AsRef<Path>>(path: P) -> Option<[RGB; 4]> {
        let img = image::open(path).ok()?;
        let img = img.resize(80, 80, image::imageops::FilterType::Nearest);
        let w = img.width();
        let h = img.height();
        if w == 0 || h == 0 { return None; }

        // Sample 4 quadrants of the image
        let mut quadrants = [(0u64, 0u64, 0u64, 0u64); 4]; // (r, g, b, count)
        let mid_x = w / 2;
        let mid_y = h / 2;

        for pixel in img.pixels() {
            let (px, py, rgba) = (pixel.0, pixel.1, pixel.2);
            let r = rgba[0] as u64;
            let g = rgba[1] as u64;
            let b = rgba[2] as u64;

            let qi = match (px < mid_x, py < mid_y) {
                (true, true) => 0,   // top-left
                (false, true) => 1,  // top-right
                (true, false) => 2,  // bottom-left
                (false, false) => 3, // bottom-right
            };
            quadrants[qi].0 += r;
            quadrants[qi].1 += g;
            quadrants[qi].2 += b;
            quadrants[qi].3 += 1;
        }

        let mut palette = [RGB::new(0.2, 0.15, 0.2); 4];
        for (i, q) in quadrants.iter().enumerate() {
            if q.3 > 0 {
                let r = (q.0 / q.3) as f64 / 255.0;
                let g = (q.1 / q.3) as f64 / 255.0;
                let b = (q.2 / q.3) as f64 / 255.0;
                // Keep vibrant colours; only lightly desaturate and darken
                palette[i] = RGB::new(r, g, b).desaturate(0.1).darken(0.65);
            }
        }
        Some(palette)
    }

    fn extract_dominant_color(img: &DynamicImage) -> RGB {
        // Resize for performance
        let img = img.resize(150, 150, image::imageops::FilterType::Nearest);
        
        let mut r_total = 0u64;
        let mut g_total = 0u64;
        let mut b_total = 0u64;
        let mut count = 0u64;

        for pixel in img.pixels() {
            let rgba = pixel.2;
            let r = rgba[0] as u64;
            let g = rgba[1] as u64;
            let b = rgba[2] as u64;

            // Skip very dark and very light pixels
            let sum = r + g + b;
            if sum < 50 || sum > 700 {
                continue;
            }

            r_total += r;
            g_total += g;
            b_total += b;
            count += 1;
        }

        if count == 0 {
            return RGB::new(0.4, 0.3, 0.35); // Default brownish color
        }

        let r_avg = (r_total / count) as f64 / 255.0;
        let g_avg = (g_total / count) as f64 / 255.0;
        let b_avg = (b_total / count) as f64 / 255.0;

        // Darken and desaturate for background
        let color = RGB::new(r_avg, g_avg, b_avg);
        color.desaturate(0.6).darken(0.3)
    }
}

fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    let s = if max == 0.0 { 0.0 } else { delta / max };
    let v = max;

    (h, s, v)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = match h as i32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    (r + m, g + m, b + m)
}
