# Vendored Beacon OpenAPI contract

- Repository: `https://github.com/MeshCore-Beacon/beacon-server`
- Commit: `4c391b849ade3e35884315126d61a468d51b24a6`
- Source: `docs/swagger.yaml`
- Documentation repository: `https://github.com/MeshCore-Beacon/beacon-docs`
- Documentation commit: `6660c56b2dbae965836556dbbf0f6c9fb1b06640`
- Documentation source: `docs/api-contract.md`
- Synchronized: 2026-11-28
- Source contract version: `2.3.0` (from pinned upstream commit)
- Note: The explicitly requested annotated `v2.3.0` tag resolves to the commit above; at synchronization time the latest published GitHub Release was still `v2.2.0`.

To update, identify the latest published Beacon release, review its server and
documentation contracts, then pin the release's full commit and run:

```sh
python3 scripts/sync-openapi.py <full-beacon-server-commit>
cargo test --manifest-path rust/Cargo.toml --locked
```

The synchronization command refuses branches and moving tags; it requires a
full 40-character commit SHA. Review the upstream handlers and update the
checked-in Rust tool discovery contract deliberately if the API changes.
