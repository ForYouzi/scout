# Scout rules

Scout-specific instructions. `AGENTS.md` comes from upstream Yaak; keep Scout rules here so upstream merges stay clean.

## Merge upstream before every release

Before pushing a `scout-v*` release tag, merge the latest upstream Yaak into `main`:

1. `git fetch upstream origin` and check `git rev-list --count main..upstream/main`. If it is 0, skip to step 4.
2. On a `sync/upstream-YYYY-MM-DD` branch from `origin/main`, run `git merge upstream/main`. Keep Scout's own files
   (see "Syncing with upstream" in `README.md`) when resolving conflicts.
3. Land the sync as a real merge commit, never a squash, so the next sync does not replay the same upstream history.
   Wait for CI (`Lint and Test`) to pass first.
4. Tag the release only on a `main` commit that already contains `upstream/main`
   (`git merge-base --is-ancestor upstream/main main`).

## GitHub CLI

This clone has an `upstream` remote pointing at `mountain-loop/yaak`. Pass `--repo ForYouzi/scout` to every `gh`
command so issues, PRs and releases never land upstream.
