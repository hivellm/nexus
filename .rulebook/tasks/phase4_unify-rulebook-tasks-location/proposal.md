# Proposal: phase4_unify-rulebook-tasks-location

## Why

The repo has both `rulebook/tasks/` and `.rulebook/tasks/` directories,
each populated with task definitions. The MCP `rulebook_task_*` tools
write to `.rulebook/`; older commits reference `rulebook/`. This is the
tail end of an in-flight migration that nobody finished. Symptoms:

- Contributors don't know which directory to edit.
- CI / hooks that grep one path miss tasks stored under the other.
- Clutter in the repo root.

## What Changes

- Decide the canonical location (almost certainly `.rulebook/`, matching
  the live MCP integration).
- Move every still-relevant task from `rulebook/tasks/` into
  `.rulebook/tasks/` preserving their history (`git mv`).
- Delete the empty `rulebook/` tree.
- Update any doc, hook, or script that still references the old path.

## Impact

- Affected specs: none
- Affected code: `rulebook/**/*`, `.rulebook/**/*`, any script or
  workflow referencing `rulebook/tasks/`
- Breaking change: NO (internal organisation)
- User benefit: one source of truth for pending work; new contributors
  stop filing tasks in the wrong tree
