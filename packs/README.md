# winr App Packs

App packs are target-specific manifests and assets that sit on top of the generic `winr-inject`, `winr-perception`, and `winr-workflows` layers.

Rules:

- generic crates should not hardcode target-specific task names, object names, or tuning constants
- app packs should describe target-specific detectors, backend preferences, and workflow defaults
- Roblox is the first expected pack, but the pack layout is intended to support other applications later
