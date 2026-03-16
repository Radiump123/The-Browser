# Budgie Browser (Linux-first fork plan)

This repository now includes a **Budgie Browser** scaffold with practical Linux-oriented building blocks that can be integrated into the existing Firefox-derived codebase.

## Rebranding targets

- Product name: **Budgie Browser**
- Binary name: `budgie-browser`
- App ID: `org.charlot.budgiebrowser`
- Linux desktop entry: `configs/linux/org.charlot.budgiebrowser.desktop`

## New modules included

### 1) Budgie Control daemon (`tools/budgie-control-daemon`)

A Rust daemon that:
- samples process CPU and memory usage
- applies process suspension (`SIGSTOP`) when a threshold is exceeded
- exports JSON snapshots to `/tmp/budgie-control-stats.json` for UI consumption
- hosts a local in-browser control panel and API on `http://127.0.0.1:47831/`
- supports Opera GX-style presets (`eco`, `balanced`, `beast`) plus custom sliders

Example:

```bash
cargo run --manifest-path tools/budgie-control-daemon/Cargo.toml -- \
  --cpu-limit 75 \
  --ram-limit-mib 768 \
  --process-name budgie-browser \
  --preset balanced

# disable per-cycle logging (for quieter runs)
# note: this is a clap bool value, so use =false
# --verbose=false

# open in Budgie Browser / Firefox / Chromium
xdg-open http://127.0.0.1:47831/
```

### 2) Theme engine (`tools/budgie-theme-engine`)

A Rust hot-reload theme loader for JSON themes stored in:

- `~/.config/budgie-browser/themes/`

Included example theme:

- `configs/themes/midnight-budgie.json`

Example:

```bash
cargo run --manifest-path tools/budgie-theme-engine/Cargo.toml -- \
  --theme configs/themes/midnight-budgie.json \
  --watch
```

### 3) Linux app bridge (`scripts/budgie_app_bridge.py`)

A Python helper for sidebar-launchable integrations (`spotify`, `discord`, `telegram`) via Flatpak.

Example:

```bash
python3 scripts/budgie_app_bridge.py spotify
```

## Linux-first integration checklist

- [x] Wayland/X11-ready desktop metadata scaffold
- [x] Flatpak launch helper for web app integrations
- [x] Runtime process control daemon for GX-style limits
- [x] JSON-based, hot-reloadable theme input path
- [ ] PipeWire capture defaults wiring in core engine
- [ ] xdg-desktop-portal permissions flow
- [ ] `.deb`, `.rpm`, and AppImage packaging pipeline

## Privacy defaults guidance

- Keep telemetry disabled by default.
- Run sidebar app surfaces in isolated contexts/profiles.
- Maintain GPL-compatible licensing for all added components.


### Budgie Control API

- `GET /status` returns current limits + sampled processes
- `POST /limits` updates live custom limits with JSON body:
- `POST /preset` applies a GX preset (`eco`, `balanced`, `beast`) instantly:

```json
{
  "preset": "balanced"
}
```

```json
{
  "cpu_limit": 70,
  "ram_limit_mib": 1536,
  "idle_cpu_threshold": 3.5
}
```


## Automation

From repository root you can now run everything using either Make or Just:

```bash
make budgie-check
make budgie-build
make budgie-run-control

# or
just budgie-check
just budgie-build
just budgie-run-control
```
