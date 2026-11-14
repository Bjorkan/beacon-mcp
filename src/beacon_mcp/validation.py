"""Beacon identifiers, UTC windows and untrusted JSON object boundaries."""

import re
import uuid
from datetime import datetime
from typing import cast

from beacon_mcp.errors import invalid


def timestamp(text: str) -> int:
    if len(text) > 24 or not text.endswith("Z") or "T" not in text:
        raise invalid("timestamp must be RFC3339 UTC")
    try:
        return int(datetime.fromisoformat(text).timestamp() * 1000)
    except ValueError:
        raise invalid("invalid UTC calendar date") from None


def duration(text: str) -> int:
    if not re.fullmatch(r"[0-9]+[mh]", text):
        raise invalid("invalid duration")
    minutes = int(text[:-1]) * (60 if text[-1] == "h" else 1)
    if not 1 <= minutes <= 43200:
        raise invalid("range must be between 1 minute and 720 hours")
    return minutes


def identifier(kind: str, value: str) -> str:
    """Validate and normalize a path identifier before URL encoding."""
    if kind == "uuid":
        try:
            parsed = uuid.UUID(value)
            if (
                len(value) == 36
                and parsed.variant == uuid.RFC_4122
                and 1 <= (parsed.version or 0) <= 8
            ):
                return str(parsed)
        except (ValueError, AttributeError):
            pass
        raise invalid("invalid UUID")
    if kind in ("hash", "tag"):
        length = 16 if kind == "hash" else 8
        if re.fullmatch(rf"[0-9a-fA-F]{{{length}}}", value):
            return value.lower()
    elif kind == "number":
        if re.fullmatch(r"\+?[0-9]+", value) and 0 < int(value) <= 2_147_483_647:
            return value
    elif kind == "iata":
        if re.fullmatch(r"[A-Za-z0-9]{3}", value):
            return value.upper()
    elif kind == "scope":
        if (
            value.strip()
            and len(value.encode()) <= 128
            and value not in (".", "..")
            and not re.search(r"[/\\%\x00-\x1f\x7f-\x9f]", value)
        ):
            return value
    raise invalid("invalid identifier")


def json_object(value: object) -> dict[str, object] | None:
    """Narrow an untrusted JSON object to string keys and opaque values."""
    if not isinstance(value, dict):
        return None
    if any(not isinstance(key, str) for key in cast(dict[object, object], value)):
        return None
    # JSON object keys were checked before the cast; their values stay untrusted.
    return cast(dict[str, object], value)
