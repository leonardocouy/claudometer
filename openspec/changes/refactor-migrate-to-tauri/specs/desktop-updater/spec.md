## ADDED Requirements

### Requirement: Signed Update Verification
The system SHALL verify the authenticity of update artifacts using a configured public signing key.

#### Scenario: Update signature is invalid
- **GIVEN** the updater has downloaded an update artifact
- **WHEN** signature verification fails
- **THEN** the system SHALL reject the update
- **AND** it SHALL surface a safe error message without crashing

### Requirement: Update Manifest Source
The system SHALL check for updates using a `latest.json` manifest hosted with the latest GitHub Release.

#### Scenario: Update manifest is reachable
- **GIVEN** the user triggers an update check (or startup check is enabled)
- **WHEN** the system fetches the update manifest
- **THEN** it SHALL compare the current version to the manifest version

### Requirement: Update Check UX
The system SHALL expose a user-triggered update check from the tray UI.

#### Scenario: User checks for updates from tray
- **GIVEN** the app is running in the tray
- **WHEN** the user selects “Check for Updates…”
- **THEN** the system SHALL check for updates and provide a result (up-to-date / update available / error)

