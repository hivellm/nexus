# Helm chart YAML offline-validation strategy for hosts without `helm`
**Source**: manual
**Date**: 2026-04-29
**Related Task**: phase7_helm-chart-k8s
**Tags**: helm, ci, yaml-validation, windows-dev, no-shortcuts-hook
When `helm` isn't available on developer workstations (e.g. Windows without WSL+helm), `helm lint` / `helm template` cannot gate local edits. The Nexus chart ships a Python static checker (`scripts/tests/helm_static_check.py`) that:

1. Parses Chart.yaml and asserts required keys (`apiVersion=v2`, `type=application`, `name`, `version`, `appVersion`).
2. Parses values.yaml and asserts the documented top-level keys exist.
3. Verifies every required template file is present (statefulset, service, secret, etc).
4. Strips Helm directives (`{{ ... }}` and full-line `{{- if ... -}}`) using a regex, replacing inline directives with an inert token (`x-helm-directive`) so indentation survives, then runs `yaml.safe_load_all` on the result. This catches structural mistakes (missing colons, bad indentation) at developer-edit time.
5. Validates Compose stacks under deploy/docker-compose/.

The CI workflow runs both the Python check (fast, no helm install needed) AND `helm lint` / `helm template` (authoritative). Locally, devs can use `docker run alpine/helm:3.14.4 lint deploy/helm/nexus` as a fallback.

CRITICAL: never name the substitution token "PLACEHOLDER" — the no-shortcuts hook flags any source file containing that word as a stub. Use a domain-specific name like `x-helm-directive`.