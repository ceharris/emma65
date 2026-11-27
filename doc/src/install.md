# Install

## Rust toolchain

Install Rust via [rustup](https://www.rust-lang.org/tools/install):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

This installs the latest stable toolchain; Emma6k uses the 2024 edition, which
requires Rust 1.85 or newer. Verify with `rustc --version` and
`cargo --version`, and see the [rustup book](https://rust-lang.github.io/rustup/)
for updating an existing installation.

## System libraries

The command-line emulator (`emma6k`, `emma65` and `emma09`) and `emma65-tracer` have no system library
dependencies beyond Rust itself. Building the rest of the workspace needs
additional development packages: `emma6k-display`, `emma6k-led-matrix`, and
`emma6k-lcd-display` need the SDL2 libraries (`emma6k-led-matrix` also needs
SDL2_gfx), and `emma6k-debugger` needs Tauri's Linux dependencies (WebKitGTK,
GTK, libayatana-appindicator, librsvg).

### Ubuntu Linux

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  libsdl2-dev \
  libsdl2-gfx-dev \
  libwebkit2gtk-4.1-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

### Fedora Linux

```bash
sudo dnf install -y \
  gcc gcc-c++ make \
  SDL2-devel \
  SDL2_gfx-devel \
  webkit2gtk4.1-devel \
  openssl-devel \
  libappindicator-gtk3-devel \
  librsvg2-devel
```

## Build and install

Build the whole workspace in release mode:

```bash
cargo build --release --workspace
```

Or build only what you need — each crate's system library requirement is
independent of the others (see above):

```bash
cargo build --release              # emma6k, emma65, emma09 and emma65-tracer only
cargo build --release -p emma6k-display
cargo build --release -p emma6k-led-matrix
cargo build --release -p emma6k-lcd-display
cargo build --release -p emma6k-debugger
```

Install the binaries onto your `PATH` (`cargo install` has no `--workspace`
flag, so each workspace member is installed with its own invocation — they
all land in the same place, `~/.cargo/bin` by default):

```bash
cargo install --path .            # emma65-tracer
cargo install --path cli          # emma6k, emma65, emma09
cargo install --path display      # emma6k-display
cargo install --path led-matrix   # emma6k-led-matrix
cargo install --path lcd-display  # emma6k-lcd-display
```

`emma6k-debugger` isn't installed this way; build it as a packaged desktop
app with `cargo tauri build` instead (see [The Debugger](the-debugger.md)). On
Linux this produces installable packages under
`target/release/bundle/` — a `.deb` and a `.rpm`:

```bash
sudo apt install ./target/release/bundle/deb/emma6k-debugger_*.deb    # Ubuntu
sudo dnf install ./target/release/bundle/rpm/emma6k-debugger-*.rpm    # Fedora
```

as well as a self-contained `.AppImage` under
`target/release/bundle/appimage/` that needs no install step — `chmod +x` it
and run it directly (or use a tool like AppImageLauncher to add it to your
desktop menu).
