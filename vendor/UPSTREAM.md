# Vendored Beacon OpenAPI contract

- Repository: `https://github.com/MeshCore-Beacon/beacon-server`
- Commit: `265a835e36931a2c39b9b08e7a78837b9cf0e343`
- Source: `docs/swagger.yaml`
- Documentation repository: `https://github.com/MeshCore-Beacon/beacon-docs`
- Documentation commit: `008f988863014c32e673c5c0176ea86db2868ccf`
- Documentation source: `docs/api-contract.md`
- Synchronized: 2026-11-12
- Source contract version: `2.2.0` (from pinned upstream commit)
- Note: The Beacon source contract is pinned to the commit above; the separately pinned documentation commit may lag behind it.

To update, identify the latest published Beacon release, review its server and
documentation contracts, then pin the release's full commit and run:

```sh
python3 scripts/sync-openapi.py <full-beacon-server-commit>
cargo test --manifest-path rust/Cargo.toml --locked
```

The synchronization command refuses branches and moving tags; it requires a
full 40-character commit SHA. Review the upstream handlers and update the
checked-in Rust tool discovery contract deliberately if the API changes.

The current latest published stable release was verified as `v2.2.0` on
2026-11-22; its annotated tag resolves to the pinned immutable commit.
Run `python3 scripts/check_upstream_release.py` to compare the pin against the
latest GitHub release and resolve its tag to a commit (detecting re-cut tags).
`.github/workflows/upstream-release.yml` runs this check every Monday and on
manual dispatch; PR CI tests the checker offline. A failed release check is a
review signal, **not** an automatic contract or tool update. Do not adopt
unreleased `dev` changes or overwrite the checked-in API contract without
reviewing handlers, tool schemas and regression tests.
