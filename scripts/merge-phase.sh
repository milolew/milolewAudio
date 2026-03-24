#!/bin/bash
set -e

PHASE=${1:?Usage: merge-phase.sh <1|2|3>}

merge_pr() {
    local branch=$1
    local pr_num=$(gh pr list --head "$branch" --json number -q '.[0].number' 2>/dev/null)
    if [ -z "$pr_num" ]; then
        echo "No PR found for $branch — skipping"
        return 1
    fi
    echo "Merging PR #$pr_num ($branch)..."
    gh pr merge "$pr_num" --squash --delete-branch
    sleep 5
    git pull origin main
}

case $PHASE in
    1)
        echo "=== Merging Phase 1 ==="
        merge_pr "fix/engine-safety"
        merge_pr "fix/ui-architecture"
        echo "Phase 1 complete."
        ;;
    2)
        echo "=== Merging Phase 2 ==="
        merge_pr "feat/ui-views"
        merge_pr "feat/integration"
        echo "Phase 2 complete."
        ;;
    3)
        echo "=== Merging Phase 3 ==="
        merge_pr "feat/e2e-testing"
        echo "Phase 3 complete. All done!"
        ;;
    *)
        echo "Usage: merge-phase.sh <1|2|3>"
        exit 1
        ;;
esac
