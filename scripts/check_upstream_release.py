"""Fail when the pinned Beacon API is behind the latest published stable release.

This is intentionally a read-only release monitor, not an API contract updater.
New releases must be reviewed before replacing the frozen Swagger/tool schemas.
"""

import json
import os
import re
import sys
from pathlib import Path
from urllib.parse import quote
from urllib.request import HTTPRedirectHandler, Request, build_opener

REPOSITORY = "MeshCore-Beacon/beacon-server"
PIN_FILE = Path(__file__).resolve().parents[1] / "vendor/UPSTREAM.md"
SHA_RE = re.compile(r"[0-9a-f]{40}\Z")
TAG_RE = re.compile(r"v\d+\.\d+\.\d+\Z")


class ReleaseDrift(Exception):
    """A newer published release or a re-cut release tag needs review."""


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *_args, **_kwargs):
        raise ValueError("Unexpected GitHub API redirect")


def get_json(path: str) -> dict:
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": "beacon-mcp-upstream-release-check",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = Request(f"https://api.github.com{path}", headers=headers)
    with build_opener(NoRedirect()).open(request, timeout=20) as response:
        body = response.read(1_048_577)
    if len(body) > 1_048_576:
        raise ValueError("GitHub response exceeds 1 MiB")
    document = json.loads(body)
    if not isinstance(document, dict):
        raise ValueError("Unexpected GitHub API response")
    return document


def pinned_release(metadata: str) -> tuple[str, str]:
    version = re.search(
        r"^- Source contract version: `(\d+\.\d+\.\d+)`", metadata, re.MULTILINE
    )
    commit = re.search(r"^- Commit: `([0-9a-f]{40})`$", metadata, re.MULTILINE)
    if version is None or commit is None:
        raise ValueError("Missing or invalid Beacon release pin in vendor/UPSTREAM.md")
    return f"v{version.group(1)}", commit.group(1)


def release_commit(tag: str, fetch=get_json) -> str:
    reference = fetch(f"/repos/{REPOSITORY}/git/ref/tags/{quote(tag, safe='')}")
    obj = reference.get("object")
    for _ in range(3):
        if not isinstance(obj, dict):
            raise ValueError("Unexpected Beacon tag reference")
        sha = obj.get("sha")
        if not isinstance(sha, str) or not SHA_RE.fullmatch(sha):
            raise ValueError("Invalid Beacon tag object SHA")
        if obj.get("type") == "commit":
            return sha
        if obj.get("type") != "tag":
            raise ValueError("Unexpected Beacon tag object type")
        obj = fetch(f"/repos/{REPOSITORY}/git/tags/{sha}").get("object")
    raise ValueError("Beacon release tag contains too many annotated tag levels")


def check_latest_pin(metadata: str, fetch=get_json) -> str:
    pinned_tag, pinned_commit = pinned_release(metadata)
    release = fetch(f"/repos/{REPOSITORY}/releases/latest")
    latest_tag = release.get("tag_name")
    if not isinstance(latest_tag, str) or not TAG_RE.fullmatch(latest_tag):
        raise ValueError("Latest Beacon release has an unexpected tag format")
    latest_commit = release_commit(latest_tag, fetch)
    if latest_tag != pinned_tag or latest_commit != pinned_commit:
        raise ReleaseDrift(
            f"Beacon MCP pins {pinned_tag} ({pinned_commit}), but the latest "
            f"published Beacon Server release is {latest_tag} ({latest_commit}). "
            "Review upstream API handlers, synchronize vendor/beacon-openapi.yaml "
            "from the release commit, and update MCP tools/tests before updating the pin."
        )
    return f"Beacon MCP is up to date with {latest_tag} ({latest_commit})."


def main() -> int:
    try:
        print(check_latest_pin(PIN_FILE.read_text(encoding="utf-8")))
    except ReleaseDrift as error:
        print(f"::error::{error}", file=sys.stderr)
        return 1
    except (OSError, ValueError) as error:
        print(f"::error::Could not verify upstream Beacon release: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
