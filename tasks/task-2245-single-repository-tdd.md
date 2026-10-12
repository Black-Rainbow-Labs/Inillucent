# One repository: Black-Rainbow-Labs/Inillucent

## Terms used in this document

| Term | Meaning |
|---|---|
| the archive | `github.com/jasonmcaffee/inillucent`, private. Development happened there until this change |
| the public repository | `github.com/Black-Rainbow-Labs/Inillucent`. Every published URL names it, and until this change it held one commit per release |
| the mirror route | the `ship.ps1` route that built a release's one commit on the public repository from the release tag's tree (`packaging/mirror-github.ps1`) |
| the cutover commit | one commit on top of the public repository's `main` whose tree is the archive's `main` at the moment of the change |
| an old branch | a branch made from the archive's history before the cutover |

## 1. Why

Jason no longer wants to use the archive. Work, commits, tags, releases and CI all go to the
public repository. Today the two have no commit in common: the archive holds about 2,000 commits of
development history and the public repository holds one commit per release, built by the mirror
route from the release tag's tree. Every release had two pushes, a snapshot step, and a set of
ordering rules (the mirror before `github`, the Go tag at the mirror's commit) that existed only
because there were two repositories.

## 2. What changes

### 2.1 History: the cutover commit, not the archive's history

The public repository keeps its own history. Its `main` and its `v*` and `packages/go/v*` tags are
what proxy.golang.org and Packagist have recorded, and a registry never changes the commit it
recorded for a version, so rewriting them is not an option. The archive's history is not pushed:
it was never public, and nothing published needs it.

The change adds one commit to the public repository's `main`:

    tree    = the tree of the archive's main after this ticket is merged
    parent  = the public repository's main
    message = "Development continues in this repository"

The tree of the public repository's `main` then equals the tree of the archive's `main`, so nothing
in the working tree changes. From here on, every commit is made on top of the cutover commit and
pushed straight to the public repository.

The archive receives one last push of its `main`, so the old history is kept somewhere other than
this machine, and then nothing more.

### 2.2 The local repository

| Before | After |
|---|---|
| remote `origin` = the archive | remote `archive` = the archive, with its push URL set to `DISABLED` so nothing can push to it by accident |
| remote `brl` = the public repository | remote `origin` = the public repository |
| `main` = the archive's history | `main` = the cutover commit. Its tree is the same, so the main checkout's working tree and index do not change |
| local `v*` tags name archive commits | local `v*` and `packages/go/v*` tags are fetched from the public repository with `--force`, so a tag here names the commit the registries recorded |

### 2.3 The release, `packaging/ship.ps1`

- **The `mirror` route is removed.** The `tag` route pushes the release commit and `v<version>`
  to `origin`, which is now the public repository, so the tag already exists there when the
  `github` route creates the release. The ordering rule that made `mirror` run before `github` is
  kept by the order of `tag` and `github`.
- **The Go module tag** `packages/go/v<version>` is made at the local `v<version>` commit and pushed
  to `origin`. It used to point at the mirror's snapshot commit, because the local commit was
  development history the public repository did not have.
- **`Get-MirrorRepo`** becomes `Get-PublicRepo` and reads `origin`. **`Test-PublishedCi`** reads
  `origin`.
- `packaging/mirror-github.ps1` is no longer called. It is left in the tree for a person to delete.

### 2.4 Other scripts

| File | Change |
|---|---|
| `packaging/ci-status.ps1` | `-Remote` defaults to `origin` |
| `packaging/nightly.ps1` | the nightly pre release is replaced on `origin` |
| `packaging/ci-evidence.ps1` | the comment that names two remotes |
| `.github/workflows/tests.yml`, `nightly.yml` | the comments that call the public repository a mirror; the `if:` that runs a job only in Black-Rainbow-Labs/Inillucent stays, so a fork does not spend a stranger's minutes |

### 2.5 Documents

`AGENTS.md` (CI and the release section: the routes table, the release mistakes about the mirror),
`agent-skills/inillucent-develop/SKILL.md` and its copies, `docs/repository.md`, and
`.claude/repo-plan.md`, which also gets the rule for old branches in section 2.6.

### 2.6 Old branches

A worktree branch made before the cutover is based on the archive's history. A fast forward merge
of it into the new `main` is refused, because the new `main` does not contain its base, and that
refusal is the protection: an ordinary merge would join the two histories and push the archive's
2,000 commits to the public repository. Such a branch is moved onto the new `main` with

    git rebase --onto main archive/main <branch>

where `archive/main` is the archive's `main` the cutover was made from. Pushing to the archive is
disabled, so `archive/main` stays at that commit, and `AGENTS.md` names it this way. Writing the
commit hash into `AGENTS.md` instead cannot work: the hash is of a commit whose tree holds the file. The
open ticket working in this repository at the time of the change is told this on its own card.

## 3. How it is checked

1. The cutover commit's tree equals the tree of the archive's `main`:
   `git rev-parse main^{tree}` before and after.
2. `git merge-base --is-ancestor <old public main> main` holds, so the public repository's history
   is kept, and the archive's root commit is not an ancestor of `main`, so its history is not
   pushed.
3. `git push origin main` is a fast forward.
4. `pwsh packaging/ship.ps1 -WhatIf` prints a plan with no `mirror` route.
5. `Invoke-Pester packaging/tests` passes.
6. `pwsh packaging/ci-status.ps1` exits 0 on the new `main`, as AGENTS.md asks of any push to the
   public repository.
7. `node tools/doc-facts/check.mjs` and `cargo test -p inillucent-compat --test tooling
   documentation::` pass.
