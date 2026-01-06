## MODIFIED Requirements

### Requirement: Secure Session Key Storage (Tauri)
The system SHALL support persisting the Claude `sessionKey` when the user opts to remember it using OS credential storage (Keychain on macOS, Secret Service on Linux).

#### Scenario: User enables “remember key”
- **GIVEN** the settings window is open
- **WHEN** the user enters a session key and enables “remember”
- **THEN** the system SHALL store the key using OS credential storage
- **AND** subsequent app launches SHALL be able to retrieve it without re-entry

### Requirement: No Session Key Leakage
The system SHALL NOT log, display, or persist the Claude `sessionKey` in plaintext outside OS credential storage.

#### Scenario: Error handling during API requests
- **GIVEN** the system performs a Claude web API request
- **WHEN** the request fails (network/unauthorized/rate-limited)
- **THEN** any user-visible errors and logs SHALL redact the session key

