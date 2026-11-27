# StatPulse — macOS System, Developer & Thermal Monitor 🚀

A high-performance, ultra-lightweight menu bar application for monitoring macOS hardware thermals, per-core temperatures, fan speeds, memory pressure & Apple Silicon DRAM bandwidth, APFS storage, listening TCP ports, AI coding agent sessions, network throughput, connected devices, and controlling fan curves on macOS.

Built with **Tauri v2, Rust, React, TypeScript, and Tailwind CSS**, with a built-in **CLI & MCP Server (`--mcp`)**.

---

## 📥 Installation

### Via Homebrew (macOS)
```bash
brew install minhtri2710/tap/statpulse
```

### Manual Download
Download the latest `.dmg` installer from [GitHub Releases](https://github.com/minhtri2710/statpulse/releases).

**Upgrading from v1.3.0 to v1.3.1+:** Export your profile from Settings before replacing the app, then import it in the new version to keep preferences and thermal rules. The bundle ID (`com.statpulse.app`) uses a separate data directory; the old one is not moved or deleted. If your MCP client uses the old executable path, copy the new configuration from Agents.

---

## ⚡ Key Features

- 🌡️ **Real-Time Hardware Telemetry**: Accurate CPU (Apple Silicon P/E Cores & Intel), GPU, and Battery metrics.
- 💻 **Per-Core Thermal Breakdown**: View individual core temperatures (`P-Core`, `E-Core`, `GPU Cluster`).
- 💨 **Smart Thermal Control**:
  - **System Auto**: Default Apple SMC thermal control.
  - **Quiet Profile**: Ultra-quiet fan acoustic curve ($50^\circ\text{C} - 85^\circ\text{C}$).
  - **Performance Profile**: Aggressive thermal dissipation ($40^\circ\text{C} - 75^\circ\text{C}$).
  - **Custom Sensor Rules**: Target specific sensors (e.g. `P-Core 1`, `GPU Core`, or `Hottest Component`) with custom temperature triggers and Min/Max Fan RPM %.
- 📈 **Real-Time Temp History Sparkline**: Ultra-smooth SVG area chart tracking temperature trends with non-scaling stroke rendering.
- ⚡ **Battery, Power & Memory Bandwidth Telemetry**: Real-time wattage draw (W), Apple Silicon Unified Memory Bandwidth (GB/s) via `IOReport` (`AMC Stats`), APFS `Strict Free` vs. `Purgeable` disk breakdown, battery percentage, temperature, and cycle count via macOS `IOKit` (`IOPowerSources`, `AppleSmartBattery`, and `AppleSmartBatteryPack` fallback).
- 📌 **Native Menu Bar, Dynamic Island & Dockless Accessory App**: Runs as a macOS Menu Bar utility (`LSUIElement` / `ActivationPolicy::Accessory`), automatically hiding from Dock and `Cmd + Tab` switcher, positioning its popover window directly below the **StatPulse** tray icon (or as a multi-display notch-aware Dynamic Island) with 13 @2x Retina Menu Bar Styles and 18 Popover Looks (including Catppuccin, Tokyo Night, Nord, Gruvbox, Rosé Pine & Dracula).
- 🔍 **Developer Observability, Local AI & MCP Server**: Inspect listening ports (`cwd` + parent launcher + container port attribution + Reveal in Finder + Open in IDE + Project Stacks), monitor SSH/`cloudflared`/`ngrok`/`kubectl`/`tailscale` tunnels, freeze/resume background processes (`SIGSTOP`/`SIGCONT`), manage Ollama VRAM models & Docker/OrbStack containers, track multi-worktree AI agent sessions & model-aware token burn rate, inspect Apple Silicon power rails & NVMe SMART health, clean descriptor-pinned developer caches, run 1-tap `⌘K` commands or custom `~/.config/statpulse/scripts/`, or run `statpulse --mcp` / `statpulse status --json`.
- 🎙️ **CoreAudio Switcher & Deep Links (`statpulse://`)**: Switch default Output/Input CoreAudio devices, toggle instant Mic Mute (`⌘⇧M` or custom shortcut), trigger actions via `statpulse://` URLs, or use the bundled **Raycast Extension** (`extensions/raycast-statpulse`).
- 🔒 **Privileged Fan Actuation Service**: Administrator-installed `launchd` helper (`/Library/PrivilegedHelperTools/com.statpulse.fan-actuation`) with active console-user authorization, hardware envelope validation, and a 5-second fail-safe heartbeat lease.
- 🎨 **Liquid Glass UI**: Premium macOS dark mode aesthetic with frameless rounded glass window, custom typography, and native window dragging.

---

## 🛠️ Architecture & Tech Stack

| Layer | Technology |
| :--- | :--- |
| **Frontend** | React 19, TypeScript, Vite, Tailwind CSS v4, Lucide Icons |
| **Backend Core** | Rust (Tauri v2), Tokio async event loop, `ProfileFiles` atomic journaled store |
| **Window Positioning** | `tauri-plugin-positioner` (`Position::TrayBottomCenter`) & native `AppKit` Dynamic Island |
| **Hardware & OS FFI** | Apple SMC FFI (`smc.c`/`smc.h`), `IOKitLib`, `macos_bridge.m` (`IOReport`, `CoreAudio`, `CoreWLAN`, `Carbon`, `UserNotifications`), `cache_permissions.c` |
| **Privileged Daemon** | Rust `fan-actuation-daemon` `launchd` service (`com.statpulse.fan-actuation`) over Unix domain socket |
| **CI/CD** | GitHub Actions CI checks (`ci.yml`) & Universal macOS App/DMG release pipeline (`build-macos.yml`) |

---

## 📦 Development & Building

### Requirements
- **macOS** 13.0 or later (Apple Silicon M1-M4 or Intel)
- **Node.js** v20+ & **npm**
- **Rust** 1.80+ & Xcode Command Line Tools

### 1. Run in Development Mode
```bash
# Install node dependencies
npm install

# Start Tauri development environment
npm run tauri dev
```

### 2. Build Production Application & DMG Bundle locally
```bash
# Run automated build script
./scripts/build-dmg.sh
```
The output `.dmg` package will be generated inside the `releases/` directory.

### 3. CI/CD & Automated GitHub Actions Build
The project includes GitHub Actions workflows:
- [`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs `npm test`, `npm run build`, `cargo fmt --check`, `cargo clippy`, `cargo test --locked`, and `src/generated` verification on `main` and PRs.
- [`.github/workflows/build-macos.yml`](.github/workflows/build-macos.yml) builds Universal macOS App (`.app` and `.dmg`) supporting Apple Silicon and Intel Macs and uploads release artifacts when a version tag (`v*`) is pushed.

For Homebrew, the cask must pin the SHA-256 of the **same Universal DMG bytes that GitHub serves**, not the app archive or a separate rebuild. Compare the release asset's `digest` with the cask's `sha256` before updating `minhtri2710/tap`; a CI rebuild can have a different digest from a local candidate of the same version.

---

## 🔒 Security & Privileges

Writing fan speed target values (`F*Tg`, `F*Md` / `F*md`, `Ftst`) on modern macOS requires root privileges. **StatPulse** uses `com.statpulse.fan-actuation` and does not send commands to older helper sockets. macOS remains the fail-safe authority:

1. **Temperature & Telemetry Reading**: Runs completely unprivileged in user-space via IOKit.
2. **Fan Actuation Service**: Prompt for administrator authorization once in **Settings -> Install Helper**. This transactionally installs `/Library/PrivilegedHelperTools/com.statpulse.fan-actuation` and `/Library/LaunchDaemons/com.statpulse.fan-actuation.plist`. The installer removes the retired helper under its original name after activating its replacement. The daemon authorizes only the active console user via `getpeereid(3)`, validates targets against hardware RPM envelopes, and automatically restores **System Auto** if the heartbeat lease expires or communication fails.

---

## 📄 Documentation

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): Detailed developer & system architecture guide.

---

## 📜 License

Distributed under the GPL-3.0 License.
