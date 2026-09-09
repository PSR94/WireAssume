#!/usr/bin/env python3
"""Minimal OrbitDesk consumer used by the WireAssume end-to-end demo.

The consumer intentionally assumes PeopleCRM's `email` field is present and non-null,
even though the provider specification declares it optional and nullable.
"""

import json
import os
import sys
import urllib.error
import urllib.request


def main() -> int:
    base_url = os.environ.get("WIREASSUME_REPLAY_BASE_URL")
    if not base_url:
        print("WIREASSUME_REPLAY_BASE_URL is required", file=sys.stderr)
        return 2

    try:
        with urllib.request.urlopen(f"{base_url}/customers/123", timeout=3) as response:
            customer = json.load(response)
    except (urllib.error.URLError, json.JSONDecodeError) as exc:
        print(f"OrbitDesk could not load customer profile: {exc}", file=sys.stderr)
        return 2

    try:
        email = customer["email"]
    except (KeyError, TypeError):
        print("OrbitDesk requires customer.email to be present", file=sys.stderr)
        return 1

    if email is None:
        print("OrbitDesk requires customer.email to be non-null", file=sys.stderr)
        return 1
    if not isinstance(email, str):
        print("OrbitDesk requires customer.email to be a string", file=sys.stderr)
        return 1

    # Empty strings are deliberately tolerated. The hidden dependency demonstrated by
    # this scenario is presence + non-nullability, not non-emptiness.
    normalized_email = email.strip().lower()
    print(f"OrbitDesk customer profile OK: {normalized_email}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
