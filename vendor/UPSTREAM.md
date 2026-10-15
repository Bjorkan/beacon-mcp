# Vendored Beacon OpenAPI contract

- Repository: `https://github.com/MeshCore-Beacon/beacon-server`
- Commit: `202acc6cc8f616819a3577c57a7009ea1a48c0dd`
- Source: `docs/swagger.yaml`
- Documentation repository: `https://github.com/MeshCore-Beacon/beacon-docs`
- Documentation commit: `9e4298d5897184f1875e3d6be97164f40d208000`
- Documentation source: `docs/api-contract.md`
- Synchronized: 2026-10-15
- Source contract version: `2.0.7` (from pinned upstream commit)
- Note: The Beacon source contract is pinned to the commit above; the separately pinned documentation commit may lag behind it.

To update, identify the latest published Beacon release, review its server and
documentation contracts, then pin the release's full commit and run:

```sh
npm run openapi:sync -- <full-beacon-server-commit>
npm run openapi:generate
npm test
```

The synchronization command refuses branches and moving tags; it requires a full 40-character commit SHA.
