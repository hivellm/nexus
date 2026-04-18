## 1. Implementation
- [ ] 1.1 Add `# syntax=docker/dockerfile:1.6` at the top of `Dockerfile` and `scripts/memtest/Dockerfile.memtest`
- [ ] 1.2 Wrap the `cargo build` RUN with `--mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/app/target`
- [ ] 1.3 (Optional) Add a `cargo-chef` prepare/cook stage to isolate dependency builds
- [ ] 1.4 Ensure CI (`.github/workflows/release-server.yml`) passes `DOCKER_BUILDKIT=1` or uses `buildx`
- [ ] 1.5 Time a cold build vs a source-only rebuild and record the numbers in the PR description

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update `docs/ACT_SETUP.md` / `docs/users/getting-started/DOCKER.md` with the new cache mount dependency
- [ ] 2.2 CI workflow is the regression test — confirm it still builds + tags releases correctly
- [ ] 2.3 Run `docker build -f Dockerfile .` twice (cold then warm) locally and paste timing
