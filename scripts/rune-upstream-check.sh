#!/bin/bash
# rune-upstream-check.sh — Show new commits from ultraworkers/claw-code
# Usage: ./scripts/rune-upstream-check.sh [--detail]

set -e

UPSTREAM="upstream"
LOCAL_BRANCH="main"

# Ensure upstream remote exists
if ! git remote | grep -q "^${UPSTREAM}$"; then
    echo "Adding upstream remote..."
    git remote add "$UPSTREAM" https://github.com/ultraworkers/claw-code.git
fi

echo "Fetching upstream..."
git fetch "$UPSTREAM" --quiet

# Count new commits
NEW_COMMITS=$(git rev-list "${LOCAL_BRANCH}..${UPSTREAM}/main" --count 2>/dev/null || echo "0")

if [ "$NEW_COMMITS" = "0" ]; then
    echo "Up to date — no new upstream commits."
    exit 0
fi

echo ""
echo "═══════════════════════════════════════════════"
echo "  $NEW_COMMITS new upstream commit(s) available"
echo "═══════════════════════════════════════════════"
echo ""

# Show commit list
git log "${LOCAL_BRANCH}..${UPSTREAM}/main" --oneline --no-merges

if [ "$1" = "--detail" ]; then
    echo ""
    echo "═══════════════════════════════════════════════"
    echo "  Changed files"
    echo "═══════════════════════════════════════════════"
    echo ""
    git diff "${LOCAL_BRANCH}...${UPSTREAM}/main" --stat
fi

echo ""
echo "To cherry-pick a commit:"
echo "  git cherry-pick <hash>"
echo ""
echo "To see full diff of a commit:"
echo "  git show <hash>"
