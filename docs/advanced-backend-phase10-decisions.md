# Advanced Backend Phase 10 Decisions

This document records the concrete Phase 10 decisions for Roblox specialization.

## Phase 10 Checklist

- [x] Treat Roblox as a specialization on top of the generic advanced backend
- [x] Add Roblox-specific detector packs for common resource nodes, prompts, and regions
- [x] Add Roblox-specific movement tuning
- [x] Add Roblox-specific task recipes for harvesting, patrolling, or object approach
- [x] Add Roblox-specific profile presets for workflows
- [x] Prevent the generic workflow engine from depending on Roblox names, assumptions, or object categories

## Core Decision

Phase 10 keeps Roblox specialization in pack data instead of pushing Roblox assumptions into `winr-workflows`, `winr-perception`, or the advanced backend contracts.

- `packs/roblox/pack.toml` declares the Roblox pack entry point
- `packs/roblox/detectors.toml` declares Roblox-oriented detector presets
- `packs/roblox/workflows.toml` declares Roblox-oriented task recipes
- `packs/roblox/movement.toml` declares Roblox-oriented controller tuning
- `packs/roblox/profile-presets.toml` declares Roblox-oriented workflow presets

That gives us Roblox-specific behavior without teaching the generic engine what a "rock", "dirt patch", or Roblox prompt is.

## Generic Loader Boundary

`winr-workflows` now provides a generic pack loader.

- `load_app_pack_from_dir(...)` loads a pack directory from TOML assets
- `AppPackBundle` carries the manifest, detector presets, task recipes, movement tuning, and profile presets
- `AppPackBundle::task_recipe(...)`, `detector(...)`, and `profile_preset(...)` provide typed lookup helpers

The loader knows how to load a pack, but it does not encode Roblox-specific workflow logic in Rust enums or hardcoded branches.

## Roblox Detector Specialization

The Roblox pack now carries concrete presets for common automation targets.

- `resource-rock-template` models a resource node as a generic `interactable`
- `dirt-region-memory` models a patrol area as a generic `region`
- `prompt-ocr` models an on-screen interaction prompt as a generic `prompt`

Those names belong to the pack and can change independently from the generic observation and workflow model.

## Roblox Movement Tuning

The Roblox pack also carries target-specific movement tuning.

- turn step
- arrival threshold
- movement pulse duration
- patrol radius
- stuck detection frame window

This keeps controller tuning close to the target pack instead of forcing one global navigation profile across very different applications.

## Roblox Task Recipes And Presets

The Roblox pack currently ships generic task kinds with Roblox-specialized data.

- an `approach` recipe for resource harvesting setup
- a `patrol_region` recipe for dirt-patch traversal
- a `wait_for_prompt` recipe for interaction gating
- profile presets for `resource-harvest` and `region-patrol`

The important architectural point is that the recipes compile into the same generic `WorkflowPlan` type as every other future target pack.

## Current Limit

Phase 10 specializes only the pack layer.

- No Roblox-only runtime executor exists yet
- No Roblox memory schema is required yet
- No Roblox-only enums were added to the generic workflow model

That is intentional. The immediate win is proving that app specialization can live in data and pack assets before deeper Roblox-specific observation or input work lands in later phases.
