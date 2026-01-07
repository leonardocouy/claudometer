#!/usr/bin/env bash
set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Get current version from package.json
get_version() {
  grep '"version"' package.json | head -1 | sed 's/.*"version": "\([^"]*\)".*/\1/'
}

# Parse semver
parse_version() {
  local version="$1"
  IFS='.' read -r MAJOR MINOR PATCH <<< "$version"
}

# Bump version based on type
bump_version() {
  local current="$1"
  local type="$2"

  parse_version "$current"

  case "$type" in
    major)
      echo "$((MAJOR + 1)).0.0"
      ;;
    minor)
      echo "${MAJOR}.$((MINOR + 1)).0"
      ;;
    patch)
      echo "${MAJOR}.${MINOR}.$((PATCH + 1))"
      ;;
    *)
      echo "$current"
      ;;
  esac
}

# Update version in all files
update_files() {
  local new_version="$1"

  # package.json
  sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"${new_version}\"/" package.json

  # src-tauri/Cargo.toml (only the package version, not dependencies)
  sed -i "0,/^version = \"[^\"]*\"/s//version = \"${new_version}\"/" src-tauri/Cargo.toml

  # src-tauri/tauri.conf.json
  sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"${new_version}\"/" src-tauri/tauri.conf.json

  echo -e "${GREEN}Updated files to v${new_version}${NC}"
}

# Main
main() {
  local bump_type="${1:-}"
  local skip_push="${2:-}"

  if [[ -z "$bump_type" ]] || [[ ! "$bump_type" =~ ^(major|minor|patch)$ ]]; then
    echo -e "${YELLOW}Usage:${NC} $0 <major|minor|patch> [--no-push]"
    echo ""
    echo "Examples:"
    echo "  $0 patch      # 1.0.0 → 1.0.1"
    echo "  $0 minor      # 1.0.0 → 1.1.0"
    echo "  $0 major      # 1.0.0 → 2.0.0"
    echo "  $0 patch --no-push  # bump without pushing"
    exit 1
  fi

  # Check for uncommitted changes
  if ! git diff --quiet || ! git diff --cached --quiet; then
    echo -e "${RED}Error: You have uncommitted changes. Commit or stash them first.${NC}"
    exit 1
  fi

  # Check we're on main branch
  local branch
  branch=$(git branch --show-current)
  if [[ "$branch" != "main" ]]; then
    echo -e "${YELLOW}Warning: You're on branch '${branch}', not 'main'.${NC}"
    read -p "Continue anyway? [y/N] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
      exit 1
    fi
  fi

  local current_version
  current_version=$(get_version)

  local new_version
  new_version=$(bump_version "$current_version" "$bump_type")

  echo -e "${YELLOW}Bumping version:${NC} v${current_version} → v${new_version}"
  echo ""

  # Confirm
  read -p "Proceed? [y/N] " -n 1 -r
  echo
  if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 1
  fi

  # Update files
  update_files "$new_version"

  # Git operations
  echo -e "${YELLOW}Creating commit and tag...${NC}"
  git add package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json
  git commit -m "chore: release v${new_version}"
  git tag "v${new_version}"

  if [[ "$skip_push" == "--no-push" ]]; then
    echo ""
    echo -e "${GREEN}Done! Version bumped to v${new_version}${NC}"
    echo -e "${YELLOW}Run 'git push && git push --tags' when ready.${NC}"
  else
    echo -e "${YELLOW}Pushing to remote...${NC}"
    git push origin "$branch"
    git push origin "v${new_version}"

    echo ""
    echo -e "${GREEN}Done! Release v${new_version} triggered.${NC}"
    echo -e "Watch the build: ${YELLOW}https://github.com/leonardocouy/claudometer/actions${NC}"
  fi
}

main "$@"
