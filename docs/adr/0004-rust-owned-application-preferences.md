---
status: accepted
---

# Use a Rust-owned Application preferences module

StatPulse will own display temperature unit, telemetry cadence, launch-at-login state, menu bar and Dynamic Island presentation, Keep Awake rules, global/mic shortcuts, alert thresholds, auto-thermal triggers, and workspace roots in a Rust Application preferences module. The module exposes one authoritative preferences snapshot and one tagged update interface; validation, atomic journaled persistence via `ProfileFiles`, `osascript` login-item reconciliation via `MacLoginItem`, and telemetry-loop cadence notification remain behind that interface. Store and autostart adapters provide hardware-independent tests, macOS remains authoritative for launch-at-login state, and Rust generates the TypeScript contracts. This adds backend state but gives settings changes locality, makes the interface the test surface, and prevents frontend controls from diverging from runtime behavior.
