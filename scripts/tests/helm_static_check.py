#!/usr/bin/env python3
"""Static structural check for the Nexus Helm chart.

Helm itself isn't always available on developer workstations — the
authoritative `helm lint` / `helm template` runs in CI
(.github/workflows/helm-lint.yml). This script gives developers
a fast offline pass that catches the most common structural mistakes:

- Chart.yaml has the required keys with the right types.
- values.yaml is parseable YAML and exposes the documented top-level
  keys.
- Every file under templates/ that is not _helpers.tpl is parseable
  YAML *after* stripping Helm directives.
- Required templates exist (statefulset, service, secret, ...).

Exit codes:
  0 — every check passed.
  1 — one or more checks failed (details printed to stderr).

Usage:
  python scripts/tests/helm_static_check.py [--chart deploy/helm/nexus]
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from typing import Iterable

import yaml

REQUIRED_TEMPLATES = {
    "statefulset.yaml",
    "service.yaml",
    "service-headless.yaml",
    "configmap.yaml",
    "secret.yaml",
    "serviceaccount.yaml",
    "networkpolicy.yaml",
    "poddisruptionbudget.yaml",
    "servicemonitor.yaml",
    "ingress.yaml",
}

REQUIRED_VALUES_KEYS = {
    "image",
    "mode",
    "cluster",
    "replicaCount",
    "server",
    "auth",
    "persistence",
    "service",
    "probes",
    "resources",
    "serviceAccount",
    "networkPolicy",
    "prometheus",
    "tls",
}

# Token used to replace inline Helm directives during YAML preflight.
# Picked so it does not collide with real chart content and is not a
# semantically meaningful keyword anywhere else.
HELM_DIRECTIVE_TOKEN = "x-helm-directive"


def fail(msg: str) -> None:
    print(f"[helm-static] FAIL: {msg}", file=sys.stderr)


def info(msg: str) -> None:
    print(f"[helm-static] {msg}")


def check_chart_yaml(chart: Path) -> bool:
    path = chart / "Chart.yaml"
    if not path.exists():
        fail(f"missing {path}")
        return False
    data = yaml.safe_load(path.read_text(encoding="utf-8"))
    ok = True
    for key in ("apiVersion", "name", "version", "appVersion", "type"):
        if key not in data:
            fail(f"Chart.yaml missing key: {key}")
            ok = False
    if data.get("apiVersion") != "v2":
        fail(f"Chart.yaml apiVersion must be v2, got {data.get('apiVersion')!r}")
        ok = False
    if data.get("type") != "application":
        fail(f"Chart.yaml type must be application, got {data.get('type')!r}")
        ok = False
    if ok:
        info(f"Chart.yaml ok ({data['name']} {data['version']} appVersion={data['appVersion']})")
    return ok


def check_values_yaml(chart: Path) -> bool:
    path = chart / "values.yaml"
    if not path.exists():
        fail(f"missing {path}")
        return False
    data = yaml.safe_load(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        fail("values.yaml is not a mapping at the top level")
        return False
    missing = REQUIRED_VALUES_KEYS - data.keys()
    if missing:
        fail(f"values.yaml missing keys: {sorted(missing)}")
        return False
    info(f"values.yaml ok ({len(data)} top-level keys)")
    return True


def strip_helm(text: str) -> str:
    """Best-effort strip of Helm directives so the file becomes parseable YAML.

    This is NOT a Helm parser — we just want to confirm the YAML
    skeleton is structurally sane (proper indentation, no obvious
    typos like missing colons). False negatives are acceptable; we
    rely on `helm lint` in CI for the authoritative pass.
    """
    out_lines = []
    for line in text.splitlines():
        # Drop full-line directives entirely (`{{- if ... -}}`).
        stripped = line.strip()
        if stripped.startswith("{{") and stripped.endswith("}}") and ":" not in stripped:
            continue
        # Replace inline directives with an inert token so indentation
        # and the surrounding YAML scaffolding survive the rewrite.
        cleaned = re.sub(r"\{\{[^}]*\}\}", HELM_DIRECTIVE_TOKEN, line)
        out_lines.append(cleaned)
    return "\n".join(out_lines)


def check_templates(chart: Path) -> bool:
    tpl_dir = chart / "templates"
    if not tpl_dir.exists():
        fail(f"missing templates directory: {tpl_dir}")
        return False
    present = {p.name for p in tpl_dir.iterdir() if p.is_file()}
    missing = REQUIRED_TEMPLATES - present
    if missing:
        fail(f"templates missing: {sorted(missing)}")
        return False

    ok = True
    yaml_files = list(tpl_dir.rglob("*.yaml"))
    for path in yaml_files:
        text = path.read_text(encoding="utf-8")
        try:
            list(yaml.safe_load_all(strip_helm(text)))
        except yaml.YAMLError as exc:
            fail(f"YAML parse error in {path.relative_to(chart)}: {exc}")
            ok = False
    if ok:
        info(f"templates/ ok ({len(yaml_files)} files)")
    return ok


def check_helpers(chart: Path) -> bool:
    helpers = chart / "templates" / "_helpers.tpl"
    if not helpers.exists():
        fail(f"missing {helpers}")
        return False
    text = helpers.read_text(encoding="utf-8")
    required_defines = {
        '"nexus.name"',
        '"nexus.fullname"',
        '"nexus.labels"',
        '"nexus.selectorLabels"',
        '"nexus.serviceAccountName"',
        '"nexus.image"',
        '"nexus.headlessServiceName"',
        '"nexus.replicas"',
        '"nexus.authSecretName"',
        '"nexus.configMapName"',
        '"nexus.shardingPeers"',
    }
    missing = {d for d in required_defines if d not in text}
    if missing:
        fail(f"_helpers.tpl missing defines: {sorted(missing)}")
        return False
    info(f"_helpers.tpl ok ({len(required_defines)} defines)")
    return True


def check_compose(repo: Path) -> bool:
    compose_root = repo / "deploy" / "docker-compose"
    expected = ["single-node", "master-replica", "v2-cluster"]
    ok = True
    for name in expected:
        path = compose_root / name / "docker-compose.yml"
        if not path.exists():
            fail(f"missing {path}")
            ok = False
            continue
        try:
            yaml.safe_load(path.read_text(encoding="utf-8"))
        except yaml.YAMLError as exc:
            fail(f"YAML parse error in {path}: {exc}")
            ok = False
    if ok:
        info(f"deploy/docker-compose/ ok ({len(expected)} stacks)")
    return ok


def main(argv: Iterable[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--chart",
        type=Path,
        default=Path("deploy/helm/nexus"),
        help="Path to the Helm chart (default: deploy/helm/nexus).",
    )
    parser.add_argument(
        "--repo",
        type=Path,
        default=Path("."),
        help="Path to the repo root (default: .).",
    )
    args = parser.parse_args(list(argv))

    chart = args.chart.resolve()
    repo = args.repo.resolve()
    if not chart.is_dir():
        fail(f"--chart {chart} is not a directory")
        return 1

    info(f"chart={chart}")
    results = [
        check_chart_yaml(chart),
        check_values_yaml(chart),
        check_helpers(chart),
        check_templates(chart),
        check_compose(repo),
    ]
    if all(results):
        info("OK")
        return 0
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
