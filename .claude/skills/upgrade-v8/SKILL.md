---
name: upgrade-v8
description: Upgrade this rusty_v8 fork to a new upstream release, carrying the v8::Locker patch and flow-build CI forward. Use when asked to upgrade/sync/merge upstream, bump to a version like v150.4.0, port the locker patch, or when the word "upgrade" appears alongside a rusty_v8/V8 version number.
---

# Upgrading this fork to a new upstream release

This fork carries a small permanent patch set on top of upstream `denoland/rusty_v8`.
Upgrading means moving that patch set onto a new upstream release **by merging, never by
re-applying the patch**.

## Non-negotiable invariants

Violating any of these has already caused real damage once. Read them before typing a command.

1. **Never fetch upstream tags.** Not `--tags`, not `git fetch upstream vX.Y.Z`.
   Upstream has 250+ release tags and our tags deliberately reuse upstream's version
   names, so fetching upstream tags will collide with and try to clobber ours.
   The remote is already configured to prevent this — keep it that way:
   ```
   remote.upstream.fetch = +refs/heads/main:refs/remotes/upstream/main
   remote.upstream.tagopt = --no-tags
   ```
2. **`main` is a pure upstream mirror.** It carries none of our work. It is only ever
   fast-forwarded, and only ever to **the exact target release tag commit** — never to
   `upstream/main`'s tip, which drifts ahead with unreleased work between releases.
3. **`locker` is permanent.** It is never deleted, never recreated, never rebased.
   It holds our patch set and the accumulated history of every upgrade merge.
4. **Never branch from `upstream/main` or from a release tag directly.** Upgrades land on
   `locker` via merge. (Older upstream release lines, e.g. `v149.3.0`/`v149.4.0`, are not
   even reachable from `main` — they were cut on upstream's separate `v149.x` branch.)
5. **We create our own tags.** Tag name is the bare upstream version (`v150.4.0`),
   pointing at **our** `locker` commit, not upstream's.

## Our patch set

Everything that makes this a fork. Verify this list still matches after every upgrade:

| Path | Change |
|---|---|
| `src/locker.rs` | **added** — the `v8::Locker` binding (~114 lines) |
| `src/binding.cc` | +10 — C++ shims for Locker |
| `src/lib.rs` | +2 — declare/export the `locker` module |
| `src/isolate.rs` | small delta — Locker-aware `OwnedIsolate` handling |
| `.github/workflows/flow-build.yml` | **added** — our build (x86_64-linux release+simdutf, gnu+musl matrix) |
| `.github/workflows/ci.yml` | **deleted** |
| `.github/workflows/release.yml` | **deleted** |
| `.github/workflows/update-v8.yml` | **deleted** |

Check it at any time with:
```bash
git diff --stat $(git merge-base locker main)..locker
```

## Procedure

Replace `v150.4.0` with the target version throughout.

### 0. Preflight

```bash
git -C . status --short                      # must be clean
git config --get-regexp '^remote\.upstream\.'   # confirm invariant 1
git config rerere.enabled true                # replay recurring conflict resolutions
```

If `upstream` is missing, add it **without tags**:
```bash
git remote add upstream https://github.com/denoland/rusty_v8.git
git config remote.upstream.tagOpt --no-tags
git config remote.upstream.fetch '+refs/heads/main:refs/remotes/upstream/main'
```

### 1. Resolve the target tag to a commit, without creating a tag

```bash
git fetch upstream                               # main only, no tags
git fetch upstream refs/tags/v150.4.0            # lands in FETCH_HEAD; creates NO local tag
TARGET=$(git rev-parse FETCH_HEAD)
git log --oneline -1 "$TARGET"                   # sanity: should say "v150.4.0 (#NNNN)"
```

If a stale local tag of that name exists pointing at *upstream's* commit (e.g. left over
from a bad `--tags` fetch), delete it now — it would block creating ours in step 5:
```bash
git tag -d v150.4.0 2>/dev/null || true
```

### 2. Pin `main` to that exact commit

```bash
git switch main
git merge --ff-only "$TARGET"
```

**If this fails**, the target is not a descendant of current `main` — that means it is a
maintenance release cut on an upstream release branch (like `v149.x`). **Stop and ask.**
Do not `reset --hard` to force it; that rewrites the mirror.

### 3. Merge `main` into `locker`

```bash
git switch locker
git merge main
```

Expected conflicts, and how to resolve them:

- **`.github/workflows/ci.yml` — modify/delete.** Upstream keeps editing a file we
  deleted (v150.4.0 touched it by +131 lines). Always resolve as *stay deleted*:
  ```bash
  git rm .github/workflows/ci.yml
  ```
  Same for `release.yml` / `update-v8.yml` if they reappear. With `rerere` enabled this
  is only manual the first time.
- **`src/binding.cc`** — upstream reworks this file; keep both their changes and our
  10 lines of Locker shims.
- **`src/isolate.rs`** — upstream refactors `OwnedIsolate` construction; re-apply our
  delta on top. Note `OwnedIsolate::drop` must **not** call `v8::Locker::IsLocked`
  (that was fix `fa6a5bf`); if a merge reintroduces such a call, drop it.
- **`Cargo.toml`** — should merge clean and take upstream's version. Confirm it reads the
  target version; we never hand-bump it.

Then verify the patch set survived intact:
```bash
git diff --stat $(git merge-base locker main)..locker   # compare against the table above
```

### 4. Verify — do NOT try to build locally

**Local `cargo check` / `cargo test` cannot validate this fork.** Per `build.rs:103`, only
`V8_FROM_SOURCE=1` compiles `src/binding.cc`. The default path downloads *upstream's*
prebuilt `librusty_v8.a` and *upstream's* `src_binding.rs` — neither contains our
`v8__Locker__*` symbols. So:

- `cargo check` never compiles our `binding.cc` shims at all.
- `cargo test` fails to **link** (undefined `v8__Locker__CONSTRUCT`) regardless of
  whether our patch is correct. A failure here tells you nothing.

Building locally with `V8_FROM_SOURCE=1` would work but needs all submodules checked out
(a multi-GB V8 fetch) and roughly an hour. Not worth it for a routine upgrade.

**The gate is CI.** `.github/workflows/flow-build.yml` sets `V8_FROM_SOURCE: true`, so it
compiles `binding.cc` with our shims and link-checks the whole thing. It triggers on the
version tag push in step 5 — so pushing the tag *is* the verification.

Cheap local checks that are worth doing first (seconds, no build):

```bash
# 1. patch set intact and nothing extra crept in
git diff --stat $(git merge-base locker main)..locker

# 2. the C++ helpers our shims depend on still exist upstream
grep -c 'construct_in_place' src/binding.cc      # expect several call sites
grep -n 'Locker' src/binding.cc                  # expect static_assert + 3 shims

# 3. the fa6a5bf fix is still in place (must print nothing)
grep -n 'IsLocked' src/isolate.rs
```

What only CI can catch: `static_assert(sizeof(v8::Locker) == sizeof(size_t) * 2)` in
`binding.cc`. That asserts V8's actual `Locker` layout (`bool, bool, Isolate*` → 16 bytes).
Historically stable, but a major V8 bump could change it — if CI fails there, update the
assertion to match the new layout rather than deleting it.

### 5. Tag and push

Only after verification passes:

```bash
git tag v150.4.0                 # our commit on locker, bare upstream version name
git push origin main
git push origin locker
git push origin v150.4.0         # explicit single tag — never `git push --tags`
```

`locker` stays. Fixes discovered later just land on `locker` and get carried by the next
upgrade merge automatically.

## What not to do

- `git fetch upstream --tags` — imported 247 tags and 44 remote-tracking branches once.
  Cleanup was `git tag -d` / `git branch -rd` in bulk.
- `git push --tags` — would push our version-named tags into a namespace that collides
  with upstream's.
- Branch-per-version (`locker-v149.4.0`). The old workflow. It required re-porting the
  patch each time and stranded post-tag fixes: tag `v149.4.0` sits 2 commits *behind*
  `origin/locker-v149.4.0`, so `fa6a5bf` and `1933caf` were not in the tag.
- Deriving "our commits" via `git merge-base locker upstream/main`. Unreliable — upstream
  release commits are not all on `main`, so this returns a far-older base and a list
  polluted with upstream's own release commits.

## History

- `origin/locker-v149.3.0` @ `4f6e2af`, `origin/locker-v149.4.0` @ `1933caf` — the old
  branch-per-version scheme, superseded by permanent `locker`.
- Tags `v149.3.0` (`4f6e2af`) and `v149.4.0` (`140fd96`) point at our commits, not
  upstream's.
