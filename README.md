# Rivetr

Rivetr is a native Rust + `eframe`/`egui` calendar application and the successor to the older Rivet Tauri project.

## Current direction

**Calendar is the product.** Tasks, Kanban, Dictionary, Contacts, and Map are currently dormant legacy surfaces and should not compete with calendar development.

The long-term product model is:

> **Rivetr Calendar is a local-first temporal information system in which events are canonical data and calendars are programmable views.**

Rivetr is being developed to consume and explore rich Taria temporal resources: dense public/institutional calendars, source-backed temporal data, research corpora, and ordinary personal scheduling.

See [CALENDAR_DIRECTION.md](CALENDAR_DIRECTION.md) for the canonical scope and development priorities.

## Current calendar capabilities

- Native `eframe`/`egui` desktop application
- Year, quarter, month, week, and day calendar views
- Calendar navigation and persisted UI state
- Local ICS import and re-import
- Imported-event reconciliation for creates, updates, and deletions
- JSON calendar-bundle import
- Calendar source coloring and filtering
- Compatibility with the existing Rivet/Taskwarrior-style datastore while the calendar data model evolves

## Repository structure

- `crates/rivet_core`: vendored Rust task engine and datastore compatibility layer
- `crates/rivet_app`: native desktop application and calendar implementation
- `assets/tags.toml`: legacy/shared tag schema
- `rivet.toml`: runtime defaults

The retained task core is infrastructure and compatibility code; it does not imply that the standalone task UI is a current product priority.

## Run

```bash
cargo run
```

The app requires a desktop session.

- Linux: `winit` requires an available Wayland or X11 display.
- Windows 11 x64: the intended target is `x86_64-pc-windows-msvc`.

## Data locations

- Legacy/task-compatible data: `RIVET_GUI_DATA` if set, otherwise the platform local data dir under `rivetr/gui_data`
- UI state: platform local data dir under `rivetr/ui-state.json`

These locations are inherited from the current implementation and may evolve as the calendar receives a dedicated canonical temporal store.

## Verification

```bash
cargo check --workspace
cargo test --workspace
```

## Windows 11 x64 check

```bash
rustup target add x86_64-pc-windows-msvc
cargo check --target x86_64-pc-windows-msvc
```
