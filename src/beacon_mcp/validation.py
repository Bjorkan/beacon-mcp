"""Beacon-specific UTC calendar and duration constraints."""

import re
from datetime import datetime

from beacon_mcp.client import invalid


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
