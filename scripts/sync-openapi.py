"""Synchronize the pinned public Beacon Swagger document from an immutable commit."""

import datetime
import re
import sys
from pathlib import Path
from urllib.request import HTTPRedirectHandler, build_opener


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *_args, **_kwargs):
        raise ValueError("Swagger source redirected unexpectedly")


def main() -> None:
    if len(sys.argv) != 2 or not re.fullmatch(r"[0-9a-f]{40}", sys.argv[1]):
        raise SystemExit(
            "Usage: python3 scripts/sync-openapi.py <full-40-character-commit>"
        )
    commit = sys.argv[1]
    url = f"https://raw.githubusercontent.com/MeshCore-Beacon/beacon-server/{commit}/docs/swagger.yaml"
    with build_opener(NoRedirect()).open(url, timeout=30) as response:
        document = response.read(5_242_881)
    if len(document) > 5_242_880:
        raise ValueError("Swagger contract exceeds 5 MiB")
    text = document.decode("utf-8")
    if 'swagger: "2.0"' not in text or "basePath: /api/v1" not in text:
        raise ValueError("Unexpected Beacon Swagger contract")
    match = re.search(r"^ {2}version: ['\"]?(\d+\.\d+\.\d+)['\"]?\s*$", text, re.M)
    if not match:
        raise ValueError("Swagger contract has no recognizable API version")
    path = Path("vendor/UPSTREAM.md")
    metadata = path.read_text()
    metadata = re.sub(r"^- Commit: .*?$", f"- Commit: `{commit}`", metadata, flags=re.M)
    metadata = re.sub(
        r"^- Synchronized: .*?$",
        f"- Synchronized: {datetime.date.today().isoformat()}",
        metadata,
        flags=re.M,
    )
    metadata = re.sub(
        r"^- Source contract version: .*?$",
        f"- Source contract version: `{match[1]}` (from pinned upstream commit)",
        metadata,
        flags=re.M,
    )
    Path("vendor/beacon-openapi.yaml").write_text(text)
    path.write_text(metadata)
    print(
        f"Synchronized Beacon Swagger from {commit}; review handlers and tool contracts before committing."
    )


if __name__ == "__main__":
    main()
