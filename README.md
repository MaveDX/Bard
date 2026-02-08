# Bard

A music player for MPD written in Rust and GTK 3. Features four-corner gradient backgrounds extracted from album art, a CAVA audio visualizer, waveform seeking, synchronized lyrics, and a frosted-glass queue sidebar.

## Features

### Visual
- **Four-corner gradient background** — a Cairo Coons-patch mesh gradient sampled from four quadrants of the album art, with noise dithering to eliminate banding
- **CAVA audio visualizer** — 24 bars rendered alongside the album art at ~30 fps, colored from the current palette (requires [CAVA](https://github.com/karlstav/cava); hidden if not installed)
- **Waveform seek bar** — full-song waveform extracted via ffmpeg, with click and drag seeking
- **Frosted-glass queue sidebar** — the queue panel blurs the gradient behind it using a multi-pass box blur
- **Theme toggle** — the 🎨 button in the top-right switches between the gradient background and your system GTK theme
- **Smooth lyrics scrolling** — active lyric line is centered with a lerp animation

### Playback
- **Now Playing view** — album art (210×210), song title/artist/album, waveform, time-synced lyrics, and playback controls
- **Library view** — lists folders under `~/Music`; double-click a folder to clear the queue, add all its songs, shuffle, and play
- **Queue sidebar** — slides in from the right; shows album art thumbnails, highlights the current track, supports search/filter, double-click to jump to a song
- **Playback controls** — play/pause, previous, next
- **Volume** — slider snapped to 5% increments, with scroll-wheel support

### Album Art
Bard searches for art in this order:
1. **Disk cache** — `~/.cache/Bard/`
2. **Folder images** — `cover.jpg`, `cover.png`, `folder.jpg`, `folder.png`, `albumart.jpg`, `albumart.png` in the song's directory
3. **Embedded art** — extracted from MP3 (id3) and FLAC (metaflac) tags, then written to the disk cache

On startup, Bard precaches album art for your entire `~/Music` library in the background.

### Lyrics
Bard loads LRC files from `~/Music/Lyrics/{Artist} - {Title}.lrc`.

Standard LRC format:
```
[00:12.50]First line of lyrics
[00:18.20]Second line
[00:23.40]Third line
```

The active line is highlighted and auto-scrolled to center.

## Dependencies

### Required
- **GTK 3** development libraries
- **MPD** (Music Player Daemon) listening on `127.0.0.1:6600`
- **mpc** (used internally for folder playback)
- **Rust** toolchain (cargo, rustc)

### Optional
- **ffmpeg** — needed for waveform extraction; without it the waveform bar shows a placeholder
- **CAVA** — needed for the audio visualizer bars; hidden if not installed (reads your `~/.config/cava/config` if present)

## Installation

### System dependencies

```bash
# Ubuntu/Debian
sudo apt install libgtk-3-dev mpd mpc ffmpeg cava

# Arch Linux
sudo pacman -S gtk3 mpd mpc ffmpeg cava

# Fedora
sudo dnf install gtk3-devel mpd mpc ffmpeg cava
```

### Rust toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### Build and run

```bash
cd bard
./build.sh                    # or: cargo build --release
./target/release/bard
```

### Install to system

```bash
./install.sh
```

This installs the binary to `~/.local/bin/bard`, a desktop entry to `~/.local/share/applications/`, and an icon to the hicolor theme. Bard will then appear in your application launcher.

To uninstall:
```bash
rm ~/.local/bin/bard
rm ~/.local/share/applications/bard.desktop
rm ~/.local/share/icons/hicolor/scalable/apps/bard.svg
```

## MPD Setup

Bard connects to `127.0.0.1:6600`. A minimal `~/.config/mpd/mpd.conf`:

```
music_directory    "~/Music"
bind_to_address    "127.0.0.1"
port               "6600"
```

```bash
systemctl --user start mpd   # or just: mpd
mpc update
```

## Music Organization

```
~/Music/
├── Artist/
│   └── Album/
│       ├── 01 - Song.mp3
│       ├── 02 - Song.flac
│       └── cover.jpg
├── Lyrics/
│   ├── Artist - Song One.lrc
│   └── Artist - Song Two.lrc
```

## Architecture

```
src/
├── main.rs              # Entry point, GTK application setup
├── ui.rs                # Window, views, controls, update loop
├── mpd_client.rs        # MPD protocol wrapper (via mpd-rs)
├── color_extractor.rs   # 4-quadrant palette extraction, HSV math
├── lyrics.rs            # LRC file parser
├── cava.rs              # CAVA subprocess manager (raw binary output)
├── waveform.rs          # ffmpeg-based waveform peak extraction
└── assets/icons/        # Embedded SVG icons (recolored at runtime)
```

## Development

```bash
cargo build              # debug build
cargo build --release    # optimized build
RUST_LOG=debug cargo run # run with logging
cargo check              # type-check without building
cargo fmt                # format code
cargo clippy             # lint
```

### Crate dependencies

- **gtk-rs / gdk-rs / cairo-rs / glib-rs** — GTK 3 bindings
- **mpd** — MPD protocol client
- **image** — image loading for color extraction
- **id3 / metaflac** — embedded album art extraction
- **regex** — LRC timestamp parsing
- **anyhow** — error handling
- **env_logger / log** — logging
- **dirs** — XDG directory resolution

## Customization

### Styling

Edit `style.css` to change fonts, colors, spacing, and transitions. It is loaded at compile time and applied globally.

### Window size

In `src/ui.rs`:
```rust
.default_width(380)
.default_height(650)
```

## Troubleshooting

**Can't connect to MPD:**
```bash
systemctl --user status mpd
mpc status
ss -tln | grep 6600
```

**No album art:**
- Place `cover.jpg` or `cover.png` in the album folder, or embed art in MP3/FLAC files
- Check `~/.cache/Bard/` for cached art

**No waveform:**
- Ensure `ffmpeg` is installed and in `$PATH`

**No CAVA bars:**
- Ensure `cava` is installed and in `$PATH`

**GTK not found during build:**
```bash
sudo apt install libgtk-3-dev    # Debian/Ubuntu
sudo pacman -S gtk3              # Arch
sudo dnf install gtk3-devel      # Fedora
```

**Lyrics not showing:**
- Place LRC files at `~/Music/Lyrics/{Artist} - {Title}.lrc`
- Timestamps must be in `[MM:SS.xx]` format
- Files must be UTF-8 encoded

## License

MIT
