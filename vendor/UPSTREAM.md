# Vendored Beacon OpenAPI contract

- Repository: `https://github.com/MeshCore-Beacon/beacon-server`
- Commit: `c7209b70433b8b127a5b1062fdfb17d4a676245c`
- Source: `docs/swagger.yaml`
- Documentation repository: `https://github.com/MeshCore-Beacon/beacon-docs`
- Documentation commit: `5a60f1e00b3e7c13c416382d4a53f4ea84b7b0f4`
- Documentation source: `docs/api-contract.md`
- Synchronized: 2026-10-04

To update, review both contracts, then run:

```sh
npm run openapi:sync -- <full-beacon-server-commit>
npm run openapi:generate
npm test
```

The synchronization command refuses branches and moving tags; it requires a full 40-character commit SHA.
