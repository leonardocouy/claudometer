## ADDED Requirements

### Requirement: User-Controlled Autostart
The system SHALL allow the user to enable or disable launching Claudometer automatically on login.

#### Scenario: User enables autostart
- **GIVEN** the user has opened settings
- **WHEN** the user enables “Start at login”
- **THEN** the system SHALL enable OS autostart for the application
- **AND** the preference SHALL persist across app restarts

#### Scenario: User disables autostart
- **GIVEN** autostart is enabled
- **WHEN** the user disables “Start at login”
- **THEN** the system SHALL disable OS autostart for the application

### Requirement: Autostart Launch Behavior
When launched via autostart, the system SHALL start in tray-only mode.

#### Scenario: Autostart launch
- **GIVEN** autostart is enabled
- **WHEN** the user logs in and the OS launches the app
- **THEN** the app SHALL appear in the tray
- **AND** it SHALL NOT automatically open the settings window

