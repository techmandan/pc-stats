# PC Stats Plugin for Omarchy

Real-time **CPU**, **RAM**, **GPU**, and **Network** telemetry directly on the Omarchy top bar, powered by a zero-dependency **native Rust engine** and paired with an interactive hardware monitor panel.

## Features
- **Native Rust Telemetry Engine (`src/main.rs`)**: Compiled release binary (`bin/pc-stats`) with zero external crate dependencies that reads kernel `/proc` and `/sys` interfaces directly in native code (~2ms per snapshot, ~1.5 MB RSS).
- **Top Bar Telemetry**: Displays live CPU utilization, RAM usage (`%` or `GB`), GPU utilization, and active network throughput (`↓` / `↑`) in a single compact widget.
- **Interactive Hardware Panel**:
  - **Processor**: CPU model, core/thread count, live frequency, temperature (`Tdie` / `Package`), load averages, per-thread real-time equalizer bars, and top CPU processes (computed natively from `/proc/[pid]/stat` deltas).
  - **Memory**: Used / available / total RAM, cached & buffers breakdown, and Swap utilization bar.
  - **Graphics**: Multi-vendor GPU support (**AMD** `amdgpu` sysfs + `libdrm`, **NVIDIA** `nvidia-smi`, and **Intel** `i915`/`xe`) with core load, VRAM usage, edge/hotspot/memory temperatures, power draw (`W`), clock speed, and fan RPM.
  - **Network**: Auto-detected primary interface (`Ethernet` or `Wi-Fi`), negotiated link speed, local IPv4 address, live download/upload rates, and session total data transferred.
  - **Instant Bar Customization**: Toggle individual metrics (`CPU`, `RAM`, `GPU`, `Network`), switch RAM between `%` and `GB`, or enable live CPU/GPU temperatures on the bar directly from the popup panel.

## Dependencies
- **Omarchy / Quickshell** desktop environment
- **Rust toolchain (`cargo`)** to compile the native telemetry binary on first setup

## Installation

Install with Omarchy's plugin manager:

```bash
omarchy plugin add https://github.com/techmandan/pc-stats.git --enable
```

On first load, the plugin automatically runs `./setup` to build the release Rust binary (`bin/pc-stats`). You can also run `./setup` manually at any time:

```bash
~/.config/omarchy/plugins/pc-stats/setup
```

## Usage
- **Left-click** on the bar widget: Open or close the PC Stats hardware panel.
- **Right-click** on the bar widget: Toggle RAM bar display between `%` and `GB`.
- **Middle-click** on the bar widget (or press `B` / `Enter` inside the panel): Open `btop`.
- **Keyboard shortcuts in panel**:
  - `R`: Refresh telemetry immediately
  - `B` / `Enter`: Launch `btop`
  - `M`: Toggle RAM `%` / `GB` on the bar
  - `N`: Cycle network format (`both`, `down`, `total`)
  - `T`: Toggle CPU & GPU temperatures on the bar
  - `1` / `2` / `3` / `4`: Toggle CPU / RAM / GPU / Network visibility on the bar

## Removal

```bash
omarchy plugin remove pc-stats
```

## Architecture
- [`src/main.rs`](src/main.rs) & [`Cargo.toml`](Cargo.toml): Zero-dependency native Rust `/proc` & `/sys` telemetry engine (`bin/pc-stats`).
- [`setup`](setup): Builds and installs the optimized release binary (`lto = true`, `strip = true`).
- [`manifest.json`](manifest.json): Omarchy plugin manifest (`schemaVersion: 1`, `kinds: ["bar-widget"]`) and user-configurable settings schema.
- [`Panel.qml`](Panel.qml): Top bar widget and interactive popup panel.
- [`Service.qml`](Service.qml): Reactive state bridge managing the streaming Rust process.
- [`Model.js`](Model.js): Pure formatting and snapshot parsing functions.
- [`PcStatsIcon.qml`](PcStatsIcon.qml): Adaptive vector microchip telemetry icon.

## License
Released under the [MIT License](LICENSE).
