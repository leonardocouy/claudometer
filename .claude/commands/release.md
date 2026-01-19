---
description: Create a new release with version bump, tag, and push
argument-hint: [patch|minor|major]
---

# Release

Automate the release process: verify CI status, bump version, create tag, and push.

## What This Command Does

1. **Verify CI**: Check that main branch CI is green
2. **Analyze**: Find latest tag and commits since then
3. **Bump Version**: Update version in all config files
4. **Release**: Commit, tag, and push to trigger release workflow

## Usage

```bash
# Interactive - will ask for bump type
/release

# Direct - specify bump type
/release patch   # 1.2.0 → 1.2.1
/release minor   # 1.2.0 → 1.3.0
/release major   # 1.2.0 → 2.0.0
```

## Implementation Steps

When this command is invoked:

### 1. Verify CI Status

Run the following command to check CI status on main branch:

```bash
gh run list --branch main --limit 3
```

**Expected**: All recent runs should show `completed` and `success`.

If CI is failing:
- Report the failing workflow to the user
- Stop and do not proceed with release

### 2. Get Current Release State

Run these commands in parallel:

```bash
# Get latest tag
git tag --list --sort=-v:refname | head -1

# Get current version from package.json
grep '"version"' package.json | head -1
```

### 3. Check for New Commits

Using the latest tag from step 2, check for commits since that tag:

```bash
git log <latest-tag>..HEAD --oneline
```

If no commits are found:
- Report "No new commits since last release (v{version})"
- Stop and do not proceed

If commits exist, display them to the user.

### 4. Determine Bump Type

If argument was provided (patch/minor/major):
- Use the provided bump type

If no argument provided:
- Use AskUserQuestion tool to ask the user which bump type:
  - **patch** - Bug fixes, small changes (1.2.0 → 1.2.1)
  - **minor** - New features, backwards compatible (1.2.0 → 1.3.0)
  - **major** - Breaking changes (1.2.0 → 2.0.0)

### 5. Calculate New Version

Parse current version (MAJOR.MINOR.PATCH) and calculate new version based on bump type:
- patch: increment PATCH (1.2.3 → 1.2.4)
- minor: increment MINOR, reset PATCH (1.2.3 → 1.3.0)
- major: increment MAJOR, reset MINOR and PATCH (1.2.3 → 2.0.0)

### 6. Update Version Files

Update version string in these three files using Edit tool:

1. **package.json** - line with `"version": "X.Y.Z"`
2. **src-tauri/tauri.conf.json** - line with `"version": "X.Y.Z"`
3. **src-tauri/Cargo.toml** - line with `version = "X.Y.Z"` (in [package] section)

Read each file first, then use Edit tool to replace the old version with new version.

### 7. Create Commit

Stage only the modified files and create commit:

```bash
git add package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json
git commit -m "chore: release v{new_version}"
```

**IMPORTANT**:
- Do NOT use `git add -A` or `git add .`
- Do NOT add Co-Authored-By footer

### 8. Create and Push Tag

Create the tag and push both commit and tag:

```bash
git tag v{new_version}
git push origin main
git push origin v{new_version}
```

### 9. Report Success

Display success message with:
- Version bump: v{old} → v{new}
- Link to watch the release workflow: `https://github.com/leonardocouy/claudometer/actions`

## Important Notes

- **NEVER proceed if CI is failing** - always verify green status first
- **NEVER use `git add -A` or `git add .`** - only stage the 3 version files
- **NEVER add AI attribution** - no Co-Authored-By in commit message
- **Verify commits exist** - don't create empty releases

## Error Handling

If any step fails:
- Report the specific command that failed
- Show the error message
- Stop and ask user how to proceed

Common errors:
- **CI failing**: "CI is not green. Please fix failing checks before releasing."
- **No commits**: "No new commits since v{version}. Nothing to release."
- **Push rejected**: "Push failed. Check if branch is up to date with remote."

## Example Output

```
Release v1.3.0

CI Status: All checks passing
Latest tag: v1.2.0
New commits since v1.2.0:
  - f3f6c31 feat(tray): add color-coded usage percentage

Version bump: patch/minor/major? → minor
Updating: 1.2.0 → 1.3.0

Updated files:
  - package.json
  - src-tauri/tauri.conf.json
  - src-tauri/Cargo.toml

Created commit: chore: release v1.3.0
Created tag: v1.3.0
Pushed to origin

Release workflow triggered!
Watch progress: https://github.com/leonardocouy/claudometer/actions
```
