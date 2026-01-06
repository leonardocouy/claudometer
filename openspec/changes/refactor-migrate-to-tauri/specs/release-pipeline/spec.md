## ADDED Requirements

### Requirement: Signed Release Artifacts
The release pipeline SHALL produce signed artifacts suitable for secure auto-updates on macOS and Linux.

#### Scenario: CI release build
- **GIVEN** a new version is released
- **WHEN** CI builds the distributables
- **THEN** the artifacts SHALL include updater signatures (`.sig`)
- **AND** the release SHALL include a `latest.json` manifest referencing those signatures

