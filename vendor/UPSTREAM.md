# Vendored Beacon OpenAPI contract

- Repository: `https://github.com/MeshCore-Beacon/beacon-server`
- Commit: `d0a55dbf6d6f5ed4345bd72c039845f6c947fd16`
- Source: `docs/swagger.yaml`
- Documentation repository: `https://github.com/MeshCore-Beacon/beacon-docs`
- Documentation commit: `008f988863014c32e673c5c0176ea86db2868ccf`
- Documentation source: `docs/api-contract.md`
- Synchronized: 2026-11-05
- Source contract version: `2.1.1` (from pinned upstream commit)
- Note: The Beacon source contract is pinned to the commit above; the separately pinned documentation commit may lag behind it.

To update, identify the latest published Beacon release, review its server and
documentation contracts, then pin the release's full commit and run:

```sh
npm run openapi:sync -- <full-beacon-server-commit>
npm run openapi:generate
npm test
```

The synchronization command refuses branches and moving tags; it requires a full 40-character commit SHA.
