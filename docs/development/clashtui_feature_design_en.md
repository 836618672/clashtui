# ClashTui feature design

ClashTui supports Mihomo (Clash.Meta). CLI, TUI and Web reuse Rust workflows for profiles, templates, activation, runtime settings and service management.

## Storage

The application directory contains config.yaml, clashtui.db, optional keymap.yaml/theme.yaml and mihomo/{profiles,templates,core_override_config.yaml}. Application data and the running core configuration may use different directories.

The database stores Mihomo Profile metadata and its current selection. When reading a legacy database with other core data, preserve clashtui.db.before-mihomo-only before rewriting it. Retain Mihomo profiles and ignore removed core records; do not uninstall existing software or remove user configuration directories.

## Profiles and templates

File, URL and Template profiles are supported. Downloads are parsed as Clash YAML. Templates expand Provider groups and proxy-group placeholders. Preview does not register a profile; generation validates with Mihomo before replacing its output and metadata. Provider inlining and proxy-assisted updates are saved options.

The Mihomo override replaces selected top-level configuration fields. Runtime API changes and explicit persistence to the override file are separate operations.

## Activation and concurrency

Activation reads the profile, applies the override, checks controller credentials, runs mihomo -t, atomically replaces the effective configuration, reloads through the API and verifies observable state. Failures attempt rollback and verify recovery. The Clash API cannot expose every credential or complete Provider payload.

Cross-process locks coordinate writes. Web and CLI saves require document revisions. An external TUI editor does not participate in that revision protocol.

## Core and platform services

HTTP/WebSocket requests use an immutable endpoint/authentication snapshot. Mutations require a supported Mihomo identity. Connection closures operate on captured IDs. Remote endpoints cannot change local services or activate local files.

Linux uses systemd/OpenRC, macOS launchd and Windows NSSM. Windows service registration and system proxy operations are platform-specific. Core selection and cross-core switching have been removed.

## Interfaces and verification

The TUI has nine tabs. Optional mihomo keymap/theme sections override common settings. The current Web interface combines the pinned original MetaCubeXD panel with a local management companion; upstream frontend integration has not been implemented.

See [architecture](architecture_en.md), [the CLI/TUI/Web matrix](../reference/cli_tui_web_parity_zh.md) and [the isolated test pipeline](../testing/test_pipeline_zh.md). Real core, service, TUN and network acceptance requires an isolated VM; mock coverage is not real-core evidence.
