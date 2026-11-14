"""Preserve exact Beacon cursors when reducing upstream pages to MCP limits."""

from beacon_mcp.handlers.tool_support import Page, page


def test_page_keeps_both_cursor_forms_including_zero_and_opaque_text() -> None:
    response: dict[str, list[int] | bool | int | str] = {
        "items": [1, 2, 3],
        "hasMore": True,
        "nextCursor": 0,
        "nextPageCursor": "v1:do-not-decode-or-rebuild",
    }
    result: Page = page(response, 2)
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


def test_page_passes_through_untrusted_cursor_and_sort_values() -> None:
    cursor: dict[str, list[int | str]] = {"position": [1, "opaque"]}
    sort: list[str] = ["upstream-specific"]
    result: Page = page(
        {"items": [None], "hasMore": True, "nextCursor": cursor, "sort": sort, "order": None},
        1,
        include_sort=True,
    )
    assert result == {
        "items": [None],
        "pagination": {"hasMore": True, "nextCursor": {"cursor": cursor}},
        "sort": sort,
        "order": None,
    }
