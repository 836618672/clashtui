# Development Documentation

This document describes the code architecture of Clashtui for developers to understand the project structure.

> This is an overview of the current code. See [feature design](clashtui_feature_design_en.md) and [project status (Chinese)](../reference/project_status_zh.md) for scope and verification.

## Tech Stack

- Language: Rust (Edition 2024)
- Build System: Cargo
- TUI Framework: ratatui + crossterm
- Async Runtime: tokio
- Config/Persistence: serde + serde_yml
- HTTP Client: minreq
- CLI Parser: clap

## Directory Structure

```
src/
├── main.rs              # Entry point: five-phase startup flow
├── cli.rs               # CLI module entry, re-exports from cli/
│   └── cli/
│       ├── handler.rs   # CLI routing (profile/service/mode/update/manage/core/web/panel)
│       ├── operations.rs # manage/core adapters to shared workflows
│       ├── widgets.rs   # CLI interactive components (Confirm, Select)
│       └── utils.rs     # Version info, shell completion generation
├── config.rs            # Config module entry, re-exports from config/
│   └── config/
│       ├── core.rs      # Mihomo-only core type, ServiceController
│       ├── database.rs  # ProfileManager (Profile persistence structures)
│       └── util.rs      # Config dir resolution, load_save! macro, file path constants
├── functions.rs         # Business logic entry, re-exports from functions/
│   └── functions/
│       ├── command.rs   # System operations (file permissions, service start/stop, directory opening)
│       ├── file.rs      # File operations (Profile import/update, Template processing)
│       ├── restful.rs   # Core REST/WebSocket, sessions, resources and metrics
│       ├── management.rs # Shared actions, revision checks and job results
│       └── web.rs       # Local management HTTP/auth (page assets in web/)
├── tui.rs               # TUI module entry, re-exports from tui/
│   └── tui/
│       ├── app.rs       # App struct, event loop (~50fps), key routing
│       ├── agent.rs     # Keymap loading (keymap.yaml)
│       ├── key.rs       # Key struct (code + modifiers)
│       ├── signals.rs   # OS signal handling
│       ├── term.rs      # Terminal raw mode enter/exit/suspend
│       ├── theme.rs     # Theme loading
│       ├── utils.rs     # Utility functions
│       ├── keymap_default.yaml  # Default key bindings
│       ├── popmsg.rs    # Popup definitions (Confirm, Input, etc.)
│       ├── widget/
│       │   ├── mod.rs   # new_type_impl_tuiwidget! macro
│       │   ├── chord.rs # Chord key handler
│       │   ├── dualtab.rs  # Dual-panel tab container
│       │   ├── fzffind.rs  # Fuzzy search component
│       │   ├── help.rs  # Help panel
│       │   ├── popmsg.rs   # Popup container (rendering + event dispatch)
│       │   └── tab.rs   # Single-panel tab container + BasicTabContent/TabContent trait
│       └── tab/
│           ├── mod.rs   # Tab enum, newtype_tab!/enum_dispatch! macros, agent macros
│           ├── status.rs     # Status tab
│           ├── files.rs      # FileTab (DualTab: Profile + Template)
│           ├── proxies.rs    # Proxies tab
│           ├── resources.rs  # Rules / Providers tabs
│           ├── connections.rs # Connections tab
│           ├── logs.rs       # Logs tab
│           ├── settings.rs   # Settings tab
│           └── srvctl.rs     # Core service control tab
```

## Startup Flow (5 Phases)

The program entry point is `src/main.rs`, which executes five phases in order:

1. **CLI Parse** — Parse command-line arguments and environment variables (`CLASHTUI_CONFIG_DIR`), handle early exits (e.g. `--generate-shell-completion`)
2. **Config Init** — Determine config directory, load `config.yaml` + `clashtui.db`, create missing directories and files
3. **TUI Init** — Load keymap (`keymap.yaml`), theme (`theme.yaml`), set terminal raw mode, register panic hook
4. **Event Loop** — Run `App::serve()`, looping to process rendering, events, and async tasks
5. **Restore Terminal** — Exit raw mode; mutations persist through business transactions. Exit no longer writes `clashtui.db` unconditionally.

If the command line has subcommands (`profile`, `service`, `mode`, `update`, `manage`, `core`, `panel`, `web`), phases 3-5 are skipped and the subcommand executes directly before exiting.

## Config System

### Config Directory Resolution

Priority from high to low:
1. `--config-dir` command-line argument
2. `CLASHTUI_CONFIG_DIR` environment variable
3. `data/` subdirectory next to the executable (portable mode)
4. `$XDG_CONFIG_HOME/clashtui`
5. `~/.config/clashtui`

### Config Loading

- `ConfigFile` — Loads core paths and service config from `config.yaml`
- `BasicInfo` — Loads API address, secret, etc. from `core_override_config.yaml`
- `ProfileManager` — Loads Profile list and current selection from `clashtui.db`
- The three are merged into a `Config` struct, globally accessible via `config::CONFIG`

### Persistence

The `load_save!` macro provides YAML loading/saving. Shared writes use process locks and atomic replacement. Edits check content revisions; database serialization has stable ordering and identical bytes are not rewritten. In `functions/file/`, `coordination.rs`, `activation.rs` and `evidence.rs` implement coordination, activation and verification.

## TUI Architecture

### Event Loop

Runs in `App::serve()`, looping at ~50fps (20ms/frame):

```
Per-frame flow:
1. Handle resize (atomic flag, processed at frame top to avoid races)
2. terminal.draw(render) — render current frame
3. sync() — advance completed async tasks
4. tokio::select! wait for next event (key/tick/resize)
5. Handle key event
```

### Key Routing (Six Layers)

Keys are processed in the following order, stopping at the first match:

| Layer | Handler | Purpose |
|-------|---------|---------|
| 0 — PopUp | `popup.handle_key_event` | Popups/dialogs hijack all keys |
| 0.5 — GlobalChord | `global_chord.handle` | Global chords (e.g. Ctrl-g c to open config dir) |
| 1 — Help | `help.dismiss` | When help panel is open, press any key to dismiss |
| 2 — Chord | `chord.handle` | Tab-level multi-key chords |
| 3 — Tab | `tabs[ti].handle_key_event` | Current tab handles keys |
| 4 — Global | `handle_global_kv` | Tab switching (1-9, Tab), quit (q, Ctrl-c), help (?) |

### TuiWidget Trait

All renderable, key-handling elements implement the `TuiWidget` trait:

- `handle_key_event(&mut self, kv: &Key)` — handle key events
- `render(&mut self, f, area)` — draw UI
- `sync(&mut self)` — advance async tasks
- `on_enter(&mut self)` / `on_leave(&mut self)` — callbacks when switching tabs

Do NOT modify state during rendering (`render` accepts `&self`). State changes should happen in `handle_key_event` or `sync` callbacks.

### Tab System

#### Single Panel (Tab)

`Tab<C>` is a generic container. `C` must implement two traits:

- `BasicTabContent` — defines `Key` enum (which keys trigger), `State` type, title
- `TabContent` — defines `init`, `handle_key_event`, `render`

#### Dual Panel (DualTab)

`DualTab<C1, C2>` is used for scenarios needing two panel switching (e.g. Files tab with Profile and Template). The two content types reference each other via `DualTabContent` / `DualTabContentMate` traits.

#### Tab Enum

The `enum_dispatch!` macro unifies all tabs into a single `Tab` enum. Each variant uses the `newtype_tab!` macro to generate a wrapper and implement `TuiWidget` and `TuiTab`.

### Async Task Model

Async I/O operations are managed via `FutureSet<C>` (i.e. `tokio::task::JoinSet`):

- Spawn async tasks in `handle_key_event` or `init` via `task_set.spawn(async { ... })`
- Completed tasks produce `Callback<C>` (i.e. `Box<dyn FnOnce(&mut C)>`)
- `sync()` advances completed callbacks each frame; state changes happen here uniformly

Error handling:
- `tri!()` macro — capture errors and display a popup to the user
- `tri!(, or_cancel)` — silently swallow errors

### Popups

Popups use a `oneshot` channel pattern:
- Call `Input::new().with_title(...).build_and_send().await` to block and wait for user input
- Popup events are managed by `PopUp::check()` and `PopUp::handle_key_event()`
- Use popups only when user input is required; simple confirm/error uses inline state display

## Core Macros

| Macro | Location | Purpose |
|-------|----------|---------|
| `tri!` | `tab/mod.rs` | Error handling in async callbacks |
| `mod_agent!` | `tab/mod.rs` | Define tab default key bindings and chord shortcuts |
| `newtype_tab!` | `tab/mod.rs` | Generate Tab wrapper, implement `TuiWidget` + `TuiTab` |
| `enum_dispatch!` | `tab/mod.rs` | Dispatch Tab enum variants to trait methods |
| `new_type_impl_tuiwidget!` | `widget/mod.rs` | Auto-implement `TuiWidget` for newtype wrappers |
| `load_save!` | `config/util.rs` | Generate `from_file()` / `to_file()` for YAML config types |

### mod_agent! Macro

Two key definition styles:
- `[KeyCode::Char('j')]` — plain character key
- `key("<C-a>")` — modifier syntax (C=Ctrl, A=Alt, S=Shift)

Supports two keymap.yaml formats:
- **Mapping**: `j: SelectDown` (simple, no description or chords)
- **Sequence**: `[{on: j, action: SelectDown, desc: "Move down"}]` (with description, supports chords)

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `customized-theme` | ☑ | Custom theme support (auto-enables `tui`) |
| `tui` | (indirect) | ratatui + crossterm + tokio dependencies |
| `deprecated` | ☐ | Deprecated functionality |

Use `#[cfg(feature = "tui")]` rather than `#[cfg(feature = "customized-theme")]` for conditional compilation, unless it's theme-specific code.

## Business Logic

The `functions/` directory provides shared business workflows and API adapters:

| Module | Responsibility |
|--------|----------------|
| `command` | System-level operations: service start/stop (systemd), file permission fixes, open directory, file editor |
| `file` | Profile management: import, update (download + parse), Template expansion, subscription type detection |
| `restful` | Mihomo REST/WebSocket: configuration, proxies, connections, resources, logs and metrics; session and capability checks |
| `management` | Shared file/template/revision/activation actions with structured results |
| `web` | Local HTTP management, token authentication, job submission/recovery; reuses shared actions |

## Build Script

`build.rs` generates a version number via git, formatted as `{CARGO_PKG_VERSION}-{git-short-hash}[-dirty]`, stored in the `CLASHTUI_VERSION` environment variable.

## Version Naming Convention

- Crate name is `clashtui` (Cargo.toml)
- Internal identifiers use `clashtui` (`CLASHTUI_VERSION`, `CLASHTUI_CONFIG_DIR`, config directory `~/.config/clashtui`)
- All environment variables, YAML keys, and user-visible strings use `clashtui`

## Adding a Tab

Approximate steps:

1. Define the content type, implement `BasicTabContent` + `TabContent` (or `DualTabContent`)
2. In `tab/mod.rs`:
   - Add `mod mytab;`
   - Use `newtype_tab!` to generate the wrapper
   - Register in `prelude`'s `enum_dispatch!` and agent_init
   - Add variant to the `Tab` enum
3. In `app.rs`:
   - Add instance to the `tabs` vec in `App::new()`
   - Review the nine current tabs and the `'1'..='9'` shortcut range; update key help
   - Call init in `prelude::agent_init`
4. If dual panel: the two content types specify each other's associated type via `DualTabContent` / `DualTabContentMate`
