#!/usr/bin/env python3
"""Check that no SDK registry is silently lagging the release train.

The Nexus SDKs (`phase13_sdk-release-trusted-publishing`) ship as ONE version
across four registries: crates.io (`nexus-graph-sdk`), npm
(`@hivehub/nexus-sdk`), PyPI (`hivehub-nexus-sdk`), and NuGet (`Nexus.SDK`).
A wire/API-affecting change should land in every language at the same version.
Twice on the sibling Thunder project a registry fell behind and *nothing
noticed* — a publish job removed for an OTP requirement, an expired API key —
and both were found late, by a person, from outside. This script exists so that
never happens silently.

## What it checks, and what it deliberately does not

The naive check — "every registry must match the repo" — is worse than nothing:
between releases the repo is *supposed* to be ahead of every registry, so it
would fail on nearly every commit and be ignored within a week. A check people
ignore is a check that will not be believed when it matters. So there are two
modes:

- **tag** (a release): every registry must match the tag. This is the moment the
  one-version promise is actually being made. The tag is `sdk-v<semver>`; the
  `sdk-v` prefix is stripped before comparing.
- **drift** (any other run): the registries must agree *with each other*. The
  repo being ahead of all of them is normal between releases and passes. One
  registry behind the others is the failure this exists to catch.

The PHP and Go SDKs are out of scope for this pipeline (they are moving to their
own repositories) and are not checked here.

Usage:
    check_published_sdk_versions.py drift
    check_published_sdk_versions.py tag sdk-v2.5.0
"""

from __future__ import annotations

import json
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent

#: crates.io rejects requests without a User-Agent, and a rejected request
#: parses as "nothing published" if the caller is careless — exactly the false
#: negative this script exists to prevent.
USER_AGENT = "nexus-sdk-release-check (https://github.com/hivellm/nexus)"

TIMEOUT = 30


class CheckError(RuntimeError):
    """A registry could not be reached or understood."""


def _get_json(url: str) -> dict:
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT) as response:
            return json.load(response)
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as exc:
        raise CheckError(f"{url}: {exc}") from exc


def _version_key(version: str) -> tuple:
    """Sort key that orders 0.10.0 after 0.9.0, unlike string order."""
    return tuple(int(part) for part in re.findall(r"\d+", version))


def latest_crates() -> str:
    data = _get_json("https://crates.io/api/v1/crates/nexus-graph-sdk")
    versions = [v["num"] for v in data.get("versions", []) if not v.get("yanked")]
    if not versions:
        raise CheckError("crates.io returned no versions for nexus-graph-sdk")
    return max(versions, key=_version_key)


def latest_npm() -> str:
    data = _get_json("https://registry.npmjs.org/@hivehub/nexus-sdk")
    versions = list(data.get("versions", {}))
    if not versions:
        raise CheckError("npm returned no versions for @hivehub/nexus-sdk")
    return max(versions, key=_version_key)


def latest_pypi() -> str:
    data = _get_json("https://pypi.org/pypi/hivehub-nexus-sdk/json")
    versions = list(data.get("releases", {}))
    if not versions:
        raise CheckError("PyPI returned no releases for hivehub-nexus-sdk")
    return max(versions, key=_version_key)


def latest_nuget() -> str:
    # NuGet's flat-container index keys packages by lowercased id.
    data = _get_json("https://api.nuget.org/v3-flatcontainer/nexus.sdk/index.json")
    versions = data.get("versions", [])
    if not versions:
        raise CheckError("NuGet returned no versions for Nexus.SDK")
    return max(versions, key=_version_key)


REGISTRIES = {
    "crates.io": latest_crates,
    "npm": latest_npm,
    "PyPI": latest_pypi,
    "NuGet": latest_nuget,
}


def repo_version() -> str:
    """The version the repository claims, from the Rust SDK manifest."""
    text = (ROOT / "sdks" / "rust" / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version\s*=\s*"([^"]+)"', text, re.MULTILINE)
    if not match:
        raise CheckError("could not read the version from sdks/rust/Cargo.toml")
    return match.group(1)


def collect() -> tuple[dict[str, str], list[str]]:
    published: dict[str, str] = {}
    errors: list[str] = []
    for name, fetch in REGISTRIES.items():
        try:
            published[name] = fetch()
        except CheckError as exc:
            errors.append(str(exc))
    return published, errors


def decide(
    mode: str, published: dict[str, str], expected: str | None = None
) -> tuple[int, list[str]]:
    """The whole judgement, separated from the network so it can be tested.

    Returns ``(exit_code, messages)``. Kept pure on purpose: the interesting
    part of this check is *when it fires*, and that has to be exercisable
    offline — a network blip must never be mistaken for a lagging registry, and
    neither must a passing test.
    """
    if mode == "tag":
        if not expected:
            return 1, ["::error::tag mode needs the expected version"]
        lagging = {n: v for n, v in published.items() if v != expected}
        if lagging:
            return 1, [
                f"::error::{name} published {version} but this release is "
                f"{expected} — the SDKs must ship one version everywhere"
                for name, version in sorted(lagging.items())
            ]
        return 0, [f"ok: every registry is at {expected}"]

    # drift: registries must agree with each other. The repo being ahead of all
    # of them is the normal state between releases and is NOT a failure — a check
    # that fires on every unreleased commit gets ignored, and an ignored check is
    # worse than none.
    distinct = set(published.values())
    if len(distinct) > 1:
        newest = max(distinct, key=_version_key)
        return 1, [
            f"::error::{name} is at {version} while others are at {newest} — "
            f"a registry is lagging the release train"
            for name, version in sorted(published.items())
            if version != newest
        ]
    if not distinct:
        return 1, ["::error::no registry reported a version"]
    return 0, [f"ok: every registry agrees at {distinct.pop()}"]


def main(argv: list[str]) -> int:
    mode = argv[1] if len(argv) > 1 else "drift"
    # Accept `sdk-vX.Y.Z`, `server-vX.Y.Z`, `vX.Y.Z`, or a bare version.
    expected = re.sub(r"^(sdk-|server-)?v", "", argv[2]) if len(argv) > 2 else None

    published, errors = collect()
    for name, version in sorted(published.items()):
        print(f"  {name:<10} {version}")
    try:
        print(f"  {'repo':<10} {repo_version()}")
    except CheckError as exc:
        print(f"  repo       (unreadable: {exc})")

    if errors:
        # A registry we could not reach is not a lagging registry. Fail — a
        # silent pass would defeat the point — but do not claim a version gap
        # that was never observed.
        for error in errors:
            print(f"::error::registry unreachable: {error}")
        return 1

    code, messages = decide(mode, published, expected)
    for message in messages:
        print(message)
    return code


if __name__ == "__main__":
    sys.exit(main(sys.argv))
