# Why Not Griffe Directly?

Griffe is the Python API extraction and breaking-change engine behind ApiBump. ApiBump does not replace Griffe at the parser level; it packages Griffe into a repo-native CI workflow.

## What Griffe already gives you

- Python source loading
- API object modeling
- Git ref loading
- breaking-change detection

## What ApiBump adds on top

- repo-level package autodetection from `pyproject.toml`
- monorepo package selection based on changed files
- conservative additive classification for `minor` recommendations
- suppression rules for intentional breakages
- stable JSON output and package summaries
- PR comments, workflow annotations, and GitHub Action wiring

## The practical difference

If you want a Python API analysis library, use Griffe directly.

If you want one command or one Action that answers “does this PR require a major, minor, or patch release?” across a repo, use ApiBump.

