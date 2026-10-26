# PC Stats Plugin for Omarchy

Real-time **CPU**, **RAM**, **GPU**, and **Network** telemetry directly on the Omarchy top bar, paired with an interactive hardware monitor panel.

## Features
- **Top Bar Telemetry**: Displays live CPU utilization, RAM usage (`%` or `GB`), GPU utilization, and active network throughput (`↓` / `↑`) in a single compact widget.
- **Interactive Hardware Panel**:
  - **Processor**: CPU model, core/thread count, live frequency, temperature (`Tdie` / `Package`), load averages, per-thread real-time equalizer bars, and top CPU processes.
  - **Memory**: Used / available / total RAM, cached & buffers breakdown, and Swap utilization bar.
  - **Graphics**: Multi-vendor GPU support (**AMD** `amdgpu` sysfs + `libdrm`, **NVIDIA** `nvidia-smi`, and **Intel** `i915`/`xe`) with core load, VRAM usage, edge/hotspot/memory temperatures, power draw (`W`), clock speed, and fan RPM.
  - **Network**: Auto-detected primary interface (`Ethernet` or `Wi-Fi`), negotiated link speed, local IPv4 address, live download/upload rates, and session total data transferred.
  - **Instant Bar Customization**: Toggle individual metrics (`CPU`, `RAM`, `GPU`, `Network`), switch RAM between `%` and `GB`, or enable live CPU/GPU temperatures on the bar directly from the popup panel.
- **Zero External Dependencies**: Uses a lightweight streaming Python 3 standard-library collector (`collect.py --watch`) reading kernel `/proc` and `/sys` interfaces directly.

## Dependencies
- **Omarchy / Quickshell** desktop environment
- **Python 3** (standard library only, pre-installed on Omarchy)

## Installation

Install with Omarchy's plugin manager:

```bash
omarchy plugin add https://github.com/techmandan/pc-stats.git --enable
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
- [`manifest.json`](manifest.json): Omarchy plugin manifest (`schemaVersion: 1`, `kinds: ["bar-widget"]`) and user-configurable settings schema.
- [`Panel.qml`](Panel.qml): Top bar widget and interactive popup panel.
- [`Service.qml`](Service.qml): Reactive state bridge managing the streaming collector process.
- [`Model.js`](Model.js): Pure formatting and snapshot parsing functions.
- [`PcStatsIcon.qml`](PcStatsIcon.qml): Adaptive vector microchip telemetry icon.
- [`collect.py`](collect.py): Zero-dependency Linux `/proc` & `/sys` hardware telemetry collector.

## License
Released under the [MIT License](LICENSE).
