"""Guard JSON parsing at byte boundaries and at the supported nesting limit."""

import json

import pytest

from beacon_mcp.json_stream import JsonDepthGuard, decode_json


def test_depth_guard_preserves_string_and_escape_state_between_chunks() -> None:
    value = {"text": 'brackets [{}] and escaped quotes " and backslash \\', "nested": [[1]]}
    data = json.dumps(value).encode()
    guard = JsonDepthGuard()

    for byte in data:
        guard.feed(bytes([byte]))

    assert guard.depth == 0
    assert not guard.quoted
    assert decode_json(data) == value


@pytest.mark.parametrize("depth", [128, 129])
def test_streaming_and_complete_json_share_the_nesting_limit(depth: int) -> None:
    data = b"[" * depth + b"0" + b"]" * depth
    guard = JsonDepthGuard()
    if depth == 128:
        guard.feed(data)
        assert decode_json(data) is not None
    else:
        with pytest.raises(ValueError, match="excessive JSON nesting"):
            guard.feed(data)
        with pytest.raises(ValueError, match="excessive JSON nesting"):
            decode_json(data)
