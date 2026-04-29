## 1. Server flag
- [ ] 1.1 Add --encrypt-at-rest to nexus-server CLI
- [ ] 1.2 Wire the flag into the storage initialisation path

## 2. Migration subcommand
- [ ] 2.1 `nexus admin encrypt-database <name>`
- [ ] 2.2 Refuse to run on already-encrypted databases
- [ ] 2.3 Idempotent: re-running on a half-finished migration resumes

## 3. Rotation subcommand
- [ ] 3.1 `nexus admin rotate-key --database <name>`
- [ ] 3.2 Surface progress to stdout

## 4. Status subcommand
- [ ] 4.1 `nexus admin encryption-status [--database <name>]`
- [ ] 4.2 Output: encryption enabled, epoch, rotation progress

## 5. Mixed-mode rejection
- [ ] 5.1 Reject mixed-mode at startup
- [ ] 5.2 Error message points at the migration command

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 6.1 Update or create documentation covering the implementation
- [ ] 6.2 Write tests covering the new behavior
- [ ] 6.3 Run tests and confirm they pass
