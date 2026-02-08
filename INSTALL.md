## Installation Guide - Bard

### Quick Install (Ubuntu/Debian)

```bash
# 1. Install system dependencies
sudo apt update
sudo apt install libgtk-3-dev mpd mpc curl build-essential

# 2. Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 3. Build the player
cd bard
chmod +x build.sh
./build.sh

# 4. Run
./target/release/bard
```

### Quick Install (Arch Linux)

```bash
# 1. Install dependencies
sudo pacman -S gtk3 mpd mpc rust

# 2. Build
cd bard
./build.sh

# 3. Run
./target/release/bard
```

### Quick Install (Fedora)

```bash
# 1. Install dependencies
sudo dnf install gtk3-devel mpd mpc rust cargo

# 2. Build
cd bard
./build.sh

# 3. Run
./target/release/bard
```

---

## Detailed Installation

### Step 1: Install System Dependencies

The player needs GTK 3 development libraries and MPD.

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install \
    libgtk-3-dev \
    libcairo2-dev \
    libgdk-pixbuf2.0-dev \
    libglib2.0-dev \
    mpd \
    mpc \
    build-essential \
    pkg-config
```

**Arch Linux:**
```bash
sudo pacman -S \
    gtk3 \
    cairo \
    gdk-pixbuf2 \
    glib2 \
    mpd \
    mpc \
    base-devel
```

**Fedora:**
```bash
sudo dnf install \
    gtk3-devel \
    cairo-devel \
    gdk-pixbuf2-devel \
    glib2-devel \
    mpd \
    mpc \
    gcc
```

### Step 2: Install Rust

If you don't have Rust installed:

```bash
# Official installer
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Follow the prompts, then reload your shell
source $HOME/.cargo/env

# Verify installation
rustc --version
cargo --version
```

Should output something like:
```
rustc 1.75.0 (or later)
cargo 1.75.0 (or later)
```

### Step 3: Setup MPD

**Create MPD config:**
```bash
mkdir -p ~/.config/mpd
mkdir -p ~/.mpd/playlists

cat > ~/.config/mpd/mpd.conf << 'EOF'
music_directory    "~/Music"
playlist_directory "~/.mpd/playlists"
db_file            "~/.mpd/database"
log_file           "~/.mpd/log"
pid_file           "~/.mpd/pid"
state_file         "~/.mpd/state"
sticker_file       "~/.mpd/sticker.sql"

bind_to_address    "127.0.0.1"
port               "6600"

audio_output {
    type    "pulse"
    name    "My Pulse Output"
}

audio_output {
    type    "fifo"
    name    "FIFO Output"
    path    "/tmp/mpd.fifo"
    format  "44100:16:2"
}
EOF
```

**Start MPD:**
```bash
# Start MPD
mpd

# Or use systemd
systemctl --user enable mpd
systemctl --user start mpd

# Verify it's running
mpc status
```

**Add music:**
```bash
# Update database
mpc update

# Add all music to queue
mpc clear
mpc add /

# Start playing
mpc play
```

### Step 4: Build the Music Player

```bash
cd bard

# Build in release mode (optimized)
cargo build --release

# Or use the build script
chmod +x build.sh
./build.sh
```

First build will take 5-10 minutes as it compiles all dependencies.
Subsequent builds are much faster.

### Step 5: Run the Player

```bash
# Run the compiled binary
./target/release/bard

# Or run with cargo
cargo run --release

# With debug logging
RUST_LOG=debug ./target/release/bard
```

---

## Post-Installation Setup

### Add Music and Lyrics

**Organize your music:**
```
~/Music/
├── Artist 1/
│   └── Album 1/
│       ├── 01 - Song.mp3
│       ├── 01 - Song.lrc
│       ├── 02 - Song.mp3
│       ├── 02 - Song.lrc
│       └── cover.jpg
```

**Update MPD database:**
```bash
mpc update
```

### Install as System Application

**Create desktop entry:**
```bash
cat > ~/.local/share/applications/bard.desktop << EOF
[Desktop Entry]
Type=Application
Name=Bard
Comment=Modern music player with dynamic gradients
Exec=/path/to/bard/target/release/bard
Icon=multimedia-audio-player
Terminal=false
Categories=AudioVideo;Audio;Player;
StartupWMClass=bard
EOF
```

Replace `/path/to/` with actual path.

**Copy binary to system:**
```bash
sudo cp target/release/bard /usr/local/bin/
```

Now you can run `bard` from anywhere!

### Create Symlink

```bash
ln -s ~/path/to/bard/target/release/bard ~/.local/bin/bard
```

Add `~/.local/bin` to PATH if not already:
```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

---

## Troubleshooting

### Build Errors

**Error: "gtk-rs requires GTK 3.22 or later"**
```bash
# Check your GTK version
pkg-config --modversion gtk+-3.0

# If too old, upgrade GTK or build from newer repos
```

**Error: "linker `cc` not found"**
```bash
# Install build tools
sudo apt install build-essential  # Ubuntu/Debian
sudo pacman -S base-devel         # Arch
sudo dnf install gcc              # Fedora
```

**Error: "Could not find glib-2.0"**
```bash
# Install glib development files
sudo apt install libglib2.0-dev
```

### Runtime Errors

**Error: "Failed to connect to MPD"**
```bash
# Check MPD is running
systemctl --user status mpd

# Start MPD
mpd

# Check connection
mpc status

# Check config
cat ~/.config/mpd/mpd.conf
```

**Error: "Could not open display"**
```bash
# Make sure you're in a graphical environment
echo $DISPLAY  # Should show something like :0 or :1

# If on a server, you need X11 forwarding
ssh -X user@server
```

### Performance Issues

**High CPU usage:**
```bash
# Build with optimizations (should be default)
cargo build --release

# Check if debug build is running
file target/release/bard  # Should NOT say "not stripped"
```

**Slow startup:**
```bash
# Clear Cargo cache and rebuild
cargo clean
cargo build --release
```

---

## Updating

```bash
# Pull latest code
git pull

# Rebuild
cargo build --release

# Or use build script
./build.sh
```

---

## Uninstalling

```bash
# Remove binary
rm -f /usr/local/bin/bard
rm -f ~/.local/bin/bard

# Remove desktop entry
rm -f ~/.local/share/applications/bard.desktop

# Remove source (if you want)
rm -rf ~/bard

# MPD and music stay intact
```

---

## Development Setup

For contributing:

```bash
# Install additional tools
cargo install cargo-watch cargo-edit

# Run with auto-reload
cargo watch -x run

# Format code
cargo fmt

# Lint code
cargo clippy

# Run tests
cargo test
```

---

## Platform-Specific Notes

### macOS

```bash
# Install dependencies with Homebrew
brew install gtk+3 mpd mpc

# May need to set PKG_CONFIG_PATH
export PKG_CONFIG_PATH="/usr/local/opt/libffi/lib/pkgconfig"

# Build
cargo build --release
```

### Windows (WSL2)

```bash
# Use Ubuntu instructions inside WSL2
# You'll need X server on Windows (like VcXsrv)

# Set DISPLAY
export DISPLAY=:0

# Then follow Ubuntu install steps
```

### Raspberry Pi

```bash
# Same as Debian, but build may take longer
# Consider increasing swap space
sudo dphys-swapfile swapoff
sudo nano /etc/dphys-swapfile  # Set CONF_SWAPSIZE=2048
sudo dphys-swapfile setup
sudo dphys-swapfile swapon

# Then build normally
cargo build --release
```

---

## Getting Help

1. Check logs: `RUST_LOG=debug cargo run`
2. Verify MPD: `mpc status`
3. Check GTK: `pkg-config --modversion gtk+-3.0`
4. Test MPD connection: `telnet localhost 6600`

For more help, check the README.md file!
