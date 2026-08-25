# Release changelogs

One file per release, named exactly after its git tag:

    changelog/libosdp-sys-v3.2.3.md
    changelog/libosdp-v0.2.2.md
    changelog/osdpctl-v0.0.2.md

The three crates version independently — `libosdp-sys` tracks the vendored C
library exactly, while `libosdp` and `osdpctl` carry their own Rust-side
semver — so each release gets its own file rather than sharing one log.

Files are scaffolded by `scripts/make_release.py prepare` and validated on
every push by `make_release.py check-changelog`, before a release by
`publish`, and again in CI against the pushed tag. The format is:

    ---
    release_crate: libosdp-sys
    release_date: 2026-07-09
    release_version: v3.2.3
    ---

    A one-paragraph subject describing the release.

    ## Fixes

    - python: Clear stale AttributeError from optional channel "id" lookup

`prepare` appends a `## Changes` section seeded from the commit log — for
`libosdp-sys` that is the C library's own log between the old and new vendored
tag. Fold those hints into `Enhancements`/`Fixes` and delete the section;
`publish` refuses to release while it is still present.
