#!/bin/bash
set -e

PHASE=${1:?Usage: merge-phase.sh <1|2|3>}

merge_pr() {
    local branch=$1
    local short_name=$(echo "$branch" | sed 's|.*/||')

    # Try exact branch name first, then worktree- prefix
    local pr_num=$(gh pr list --head "$branch" --json number -q '.[0].number' 2>/dev/null)
    if [ -z "$pr_num" ]; then
        pr_num=$(gh pr list --head "worktree-$short_name" --json number -q '.[0].number' 2>/dev/null)
    fi

    if [ -z "$pr_num" ]; then
        echo ""
        echo "WARNING: No PR found for '$branch' (also tried 'worktree-$short_name')"
        echo ""
        echo "Open PRs:"
        gh pr list --limit 20
        echo ""
        echo "To merge manually:  gh pr merge <PR_NUMBER> --squash --delete-branch"
        return 1
    fi

    echo "Merging PR #$pr_num ($branch)..."
    gh pr merge "$pr_num" --squash --delete-branch
    sleep 5
    git pull origin main
}

case $PHASE in
    1)
        echo "=== Phase 1: Foundation (undo, clips, tracks) ==="
        merge_pr "feat/undo-system"
        merge_pr "feat/clip-operations"
        merge_pr "feat/track-management"
        echo "Phase 1 complete."
        ;;
    2)
        echo "=== Phase 2: Interaction (shortcuts, recording) ==="
        merge_pr "feat/keyboard-shortcuts"
        merge_pr "feat/recording-flow"
        echo "Phase 2 complete."
        ;;
    3)
        echo "=== Phase 3: Polish ==="
        merge_pr "feat/polish"
        echo "Phase 3 complete. All done!"
        ;;
    *)
        echo "Usage: merge-phase.sh <1|2|3>"
        echo ""
        echo "Phase 1: feat/undo-system → feat/clip-operations → feat/track-management"
        echo "Phase 2: feat/keyboard-shortcuts → feat/recording-flow"
        echo "Phase 3: feat/polish"
        exit 1
        ;;
esac
