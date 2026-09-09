#!/usr/bin/env python3
"""Verify that the demo was discovered experimentally, not hard-coded."""

import glob
import json
import sys


def fail(message: str) -> int:
    print(f"demo verification failed: {message}", file=sys.stderr)
    return 1


def main() -> int:
    reports = sorted(glob.glob(".wireassume/runs/run_*/report.json"))
    locks = sorted(glob.glob(".wireassume/runs/run_*/consumption.lock.json"))
    if len(reports) != 1 or len(locks) != 1:
        return fail(f"expected one run report and lock, found {len(reports)} reports / {len(locks)} locks")

    with open(reports[0], encoding="utf-8") as handle:
        report = json.load(handle)
    with open(locks[0], encoding="utf-8") as handle:
        lock = json.load(handle)

    if report["baseline"]["summary"].find("passed") < 0:
        return fail("baseline oracle did not pass")
    if report["failures"] < 2:
        return fail("expected at least the missing-email and null-email experiments to fail")
    if report["passes"] < 1:
        return fail("expected at least one harmless mutation to pass")

    by_target = {item["target"]["path"]: item for item in lock["requirements"]}
    email = by_target.get("/email")
    if not email:
        return fail("no experimentally inferred /email requirement")
    if email.get("presence") != "required":
        return fail(f"email presence was {email.get('presence')!r}, expected 'required'")
    if email.get("nullable") is not False:
        return fail(f"email nullable was {email.get('nullable')!r}, expected false")
    if email.get("accepted_empty") is not True:
        return fail(f"email accepted_empty was {email.get('accepted_empty')!r}, expected true")
    if len(email.get("evidence_refs", [])) < 3:
        return fail("email requirement is missing mutation evidence")

    email_assumptions = [
        assumption
        for assumption in lock["assumptions"]
        if assumption["target"]["path"] == "/email"
    ]
    assumption_types = {assumption["type"] for assumption in email_assumptions}
    if not {"presence", "nullability"}.issubset(assumption_types):
        return fail(f"missing email assumptions; observed {sorted(assumption_types)}")
    if any(not assumption.get("evidence_refs") for assumption in email_assumptions):
        return fail("an email assumption has no evidence references")

    print(
        "demo verification OK: OrbitDesk experimentally requires PeopleCRM email "
        "presence + non-nullability while tolerating empty strings"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
