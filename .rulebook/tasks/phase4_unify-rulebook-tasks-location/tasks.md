## 1. Implementation
- [ ] 1.1 Inventory differences: run `diff -r rulebook/tasks .rulebook/tasks` and list tasks exclusive to each tree
- [ ] 1.2 For each task only in `rulebook/`, decide: migrate to `.rulebook/` (still relevant) or archive (obsolete)
- [ ] 1.3 `git mv` survivors into `.rulebook/tasks/`
- [ ] 1.4 Remove the now-empty `rulebook/` directory
- [ ] 1.5 Grep docs + `.github/workflows/` + scripts for `rulebook/tasks` path references and update each

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update `AGENTS.md` / `CLAUDE.md` to only reference `.rulebook/` as the task location
- [ ] 2.2 No new tests required (layout change); run `rulebook_task_list` and confirm the migrated tasks appear
- [ ] 2.3 Run `cargo test --workspace` to catch any scripts/tests that had the old path hard-coded
