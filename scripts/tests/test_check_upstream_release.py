"""Offline tests for upstream release and re-cut tag detection."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from check_upstream_release import ReleaseDrift, check_latest_pin, pinned_release  # noqa: E402

COMMIT = "a" * 40
TAG_OBJECT = "b" * 40
NEW_COMMIT = "c" * 40
METADATA = (
    "# Vendored Beacon contract\n"
    f"- Commit: `{COMMIT}`\n"
    "- Source contract version: `2.2.0` (from pinned upstream commit)\n"
)
PREFIX = "/repos/MeshCore-Beacon/beacon-server"


def backend(*, version="v2.2.0", commit=COMMIT, annotated=True):
    responses = {
        f"{PREFIX}/releases/latest": {"tag_name": version},
        f"{PREFIX}/git/ref/tags/{version}": {
            "object": {"type": "tag" if annotated else "commit", "sha": TAG_OBJECT if annotated else commit}
        },
    }
    if annotated:
        responses[f"{PREFIX}/git/tags/{TAG_OBJECT}"] = {
            "object": {"type": "commit", "sha": commit}
        }
    return responses.__getitem__


class ReleaseCheckTests(unittest.TestCase):
    def test_current_annotated_tag(self):
        self.assertIn("up to date", check_latest_pin(METADATA, backend()))

    def test_current_lightweight_tag(self):
        self.assertIn("up to date", check_latest_pin(METADATA, backend(annotated=False)))

    def test_detects_new_release(self):
        with self.assertRaisesRegex(ReleaseDrift, "v2.3.0"):
            check_latest_pin(METADATA, backend(version="v2.3.0", commit=NEW_COMMIT))

    def test_detects_recut_tag_without_version_change(self):
        with self.assertRaisesRegex(ReleaseDrift, "latest published"):
            check_latest_pin(METADATA, backend(commit=NEW_COMMIT))

    def test_requires_both_pins(self):
        with self.assertRaisesRegex(ValueError, "Missing or invalid"):
            pinned_release("- Commit: `not-a-sha`\n- Source contract version: `2.2.0`\n")

    def test_rejects_invalid_release_tag(self):
        with self.assertRaisesRegex(ValueError, "unexpected tag format"):
            check_latest_pin(METADATA, backend(version="v2.3.0-rc1"))

    def test_rejects_invalid_tag_response(self):
        with self.assertRaisesRegex(ValueError, "Invalid Beacon tag object SHA"):
            check_latest_pin(METADATA, lambda path: {"tag_name": "v2.2.0"} if path.endswith("latest") else {"object": {"type": "commit", "sha": "invalid"}})


if __name__ == "__main__":
    unittest.main()
