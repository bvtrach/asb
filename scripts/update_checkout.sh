#!/usr/bin/env bash
set -euo pipefail

# Older builds generated these tracked documents in place. Preserve every
# changed copy, including staged and untracked files, before pulling. Never
# stash unrelated project files and never discard or automatically apply the
# backup over the new upstream version.
generated=(
  docs/asb-type-diagram.dot docs/asb-type-diagram.svg
  docs/asb-type-diagram.mmd docs/asb-type-diagram.html
  docs/asb-dataflow.dot docs/asb-dataflow.svg docs/asb-dataflow.html
  docs/asb-control-flow.dot docs/asb-control-flow.svg docs/asb-control-flow.html
)
present=()
for path in "${generated[@]}"; do
  if git ls-files --error-unmatch -- "$path" >/dev/null 2>&1 || [[ -f "$path" || -L "$path" ]]; then
    present+=("$path")
  fi
done

if ((${#present[@]})) && [[ -n "$(git status --porcelain=v1 --untracked-files=all -- "${present[@]}")" ]]; then
  printf '%s\n' 'Saving existing generated diagram changes in a Git stash before updating.'
  git -c user.name='ASB update' -c user.email='asb-update@localhost' stash push --include-untracked -m 'asb update: saved generated diagrams' -- "${present[@]}"
  printf '%s\n' 'Your saved changes remain in git stash; inspect with: git stash list'
fi

git pull --ff-only origin main
