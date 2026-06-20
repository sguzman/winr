# winr Advanced Backend Architecture

This document describes the current crate boundaries for the advanced backend scaffold.

## Standard backend

- `winr-core`
  - Windows window discovery and actions
  - screenshots
  - foreground input
  - message input
  - UI Automation

## Advanced backend

- `winr-inject`
  - advanced backend routing
  - session lifecycle
  - protocol envelopes
  - attachable-target discovery

- `winr-perception`
  - generic observation frame model
  - detector descriptors
  - entity observations
  - source-kind abstraction

- `winr-workflows`
  - generic workflow task and intent model
  - workflow planning contracts
  - generic app-pack manifest registry

## App-specific layer

- `packs/`
  - target-specific manifests and assets
  - backend preferences
  - detector packs
  - future task recipes and tuning

Current example:

- `packs/roblox/pack.toml`

## Separation rule

Target-specific logic should live in app packs or target-specific adapters, not inside the generic `winr-perception` or `winr-workflows` crates.
