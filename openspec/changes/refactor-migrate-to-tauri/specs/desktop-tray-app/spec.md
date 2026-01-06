## MODIFIED Requirements

### Requirement: Tray-First Application (Tauri)
The system SHALL run as a tray-first desktop application on macOS and Linux using Tauri and the system WebView.

#### Scenario: App starts and exposes tray menu without a primary window
- **GIVEN** the user launches the application
- **WHEN** the app initializes
- **THEN** a tray icon SHALL appear with a context menu
- **AND** the app SHALL NOT require any primary window to be visible

### Requirement: Settings Window On Demand
The system SHALL open a small settings window on demand from the tray menu.

#### Scenario: User opens settings from tray
- **GIVEN** the tray icon is visible
- **WHEN** the user selects “Open Settings…”
- **THEN** the system SHALL open a settings window
- **AND** the tray-first app SHALL continue running if the settings window is closed

