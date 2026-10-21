# Releases and consuming `athena-sim`

`athena` releases through release-plz, following the workspace convention in
[`wrightkit/.github` `docs/release-engineering.md`](https://github.com/wrightkit/.github/blob/main/docs/release-engineering.md):
pushes to `main` keep a release PR open, and merging it (an owner action)
creates the canonical `v{{ version }}` tag and a GitHub release. One owner
performs each external state transition.

`athena-sim` is `publish = false` and is **not** published to crates.io.
Consumers depend on it as a git dependency pinned to the release tag:

```toml
[dependencies]
athena-sim = { git = "https://github.com/wrightkit/athena", tag = "v0.1.0" }
```

This is the dependency form `wright` uses once `wrightkit/wright#833` lands.
