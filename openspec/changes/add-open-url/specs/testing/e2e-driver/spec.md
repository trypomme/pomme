# Spec Delta

## ADDED Requirements

### Requirement: Opening a link from a test

The driver SHALL offer `open_url`, which opens a link in the running app
as the window's Open URL does, and returns once the app is idle. A link the
app does not handle SHALL fail the step with the reason.

#### Scenario: Deep link in a test

- **WHEN** a test drives hello-expo, opens `pommehello:///tabs` and waits for the tabs screen's text
- **THEN** the wait succeeds
