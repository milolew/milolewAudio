#!/bin/bash
echo "=== milolew Audio — Parallel Work Dashboard ==="
echo "Time: $(date '+%H:%M:%S')"
echo ""
for wt in $(git worktree list --porcelain | grep "^worktree " | cut -d' ' -f2-); do
    branch=$(git -C "$wt" branch --show-current 2>/dev/null || echo "detached")
    ahead=$(git -C "$wt" rev-list main..HEAD --count 2>/dev/null || echo "?")
    last=$(git -C "$wt" log -1 --format="%ar — %s" 2>/dev/null || echo "no commits")
    echo "  [$branch] ${ahead} ahead | $last"
done
echo ""
if command -v gh &>/dev/null; then
    echo "=== Open PRs ==="
    gh pr list --json number,title,state,statusCheckRollup \
      --template '{{range .}}  PR #{{.number}}: {{.title}} ({{.state}}){{"\n"}}{{end}}' 2>/dev/null || echo "  (gh CLI not configured)"
fi
