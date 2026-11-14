"""Preserve exact Beacon cursors when reducing upstream pages to MCP limits."""

from beacon_mcp.tool_support import page


def test_page_keeps_both_cursor_forms_including_zero_and_opaque_text() -> None:
    response = {
        "items": [1, 2, 3],
        "hasMore": True,
        "nextCursor": 0,
        "nextPageCursor": "v1:do-not-decode-or-rebuild",
    }
    result = page(response, 2)
    assert result == {
        "items": [1, 2],
        "pagination": {
            "hasMore": True,
            "nextCursor": {"cursor": 0, "pageCursor": "v1:do-not-decode-or-rebuild"},
        },
    }
    assert response["items"] == [1, 2, 3]


def test_oversized_plain_list_does_not_invent_a_cursor() -> None:
    assert page([1, 2, 3], 2) == {"items": [1, 2], "pagination": {"hasMore": True}}
