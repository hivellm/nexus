"""Unit tests for `check_published_sdk_versions.py`.

The interesting part of the release check is *when it fires* — a network blip
must never be mistaken for a lagging registry, and a real gap must never pass.
`decide()` is pure (no network) precisely so that judgement is exercisable
offline; these tests pin every branch. Run: `pytest scripts/ci/`.
"""

from __future__ import annotations

import importlib.util
import re
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "check_published_sdk_versions",
    Path(__file__).with_name("check_published_sdk_versions.py"),
)
assert _SPEC and _SPEC.loader
chk = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(chk)


ALL = {"crates.io": "2.5.0", "npm": "2.5.0", "PyPI": "2.5.0", "NuGet": "2.5.0"}


# ── tag mode: every registry must match the release ──────────────────────────


def test_tag_all_match_passes():
    code, msgs = chk.decide("tag", ALL, "2.5.0")
    assert code == 0
    assert any("every registry is at 2.5.0" in m for m in msgs)


def test_tag_one_lagging_fails():
    published = {**ALL, "NuGet": "2.4.0"}
    code, msgs = chk.decide("tag", published, "2.5.0")
    assert code == 1
    assert any("NuGet published 2.4.0" in m and "2.5.0" in m for m in msgs)


def test_tag_without_expected_version_fails():
    code, msgs = chk.decide("tag", ALL, None)
    assert code == 1
    assert any("needs the expected version" in m for m in msgs)


# ── drift mode: registries must agree with each other ────────────────────────


def test_drift_all_agree_passes():
    code, msgs = chk.decide("drift", ALL)
    assert code == 0
    assert any("every registry agrees at 2.5.0" in m for m in msgs)


def test_drift_one_behind_fails_and_names_it():
    published = {"crates.io": "2.5.0", "npm": "2.5.0", "PyPI": "2.5.0", "NuGet": "2.4.0"}
    code, msgs = chk.decide("drift", published)
    assert code == 1
    assert any("NuGet is at 2.4.0" in m and "2.5.0" in m for m in msgs)


def test_drift_orders_versions_numerically_not_lexically():
    # 2.10.0 must be recognised as newer than 2.9.0 (string order would not).
    published = {"crates.io": "2.10.0", "npm": "2.9.0"}
    code, msgs = chk.decide("drift", published)
    assert code == 1
    assert any("npm is at 2.9.0" in m and "2.10.0" in m for m in msgs)


def test_drift_empty_fails():
    code, msgs = chk.decide("drift", {})
    assert code == 1
    assert any("no registry reported a version" in m for m in msgs)


# ── version-key ordering + prefix stripping ──────────────────────────────────


def test_version_key_numeric_ordering():
    versions = ["2.9.0", "2.10.0", "2.5.0"]
    assert max(versions, key=chk._version_key) == "2.10.0"


def test_tag_prefix_stripping():
    strip = lambda t: re.sub(r"^(sdk-|server-)?v", "", t)
    assert strip("sdk-v2.5.0") == "2.5.0"
    assert strip("server-v3.0.0") == "3.0.0"
    assert strip("v2.5.0") == "2.5.0"
    assert strip("2.5.0") == "2.5.0"
