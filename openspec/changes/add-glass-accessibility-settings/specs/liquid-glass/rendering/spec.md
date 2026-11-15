# Spec Delta

## ADDED Requirements

### Requirement: Reduce Transparency makes glass frosted

When Reduce Transparency is on, pomme SHALL draw all glass (glass views and
bars) with the look measured on the iOS 26 Simulator with that setting on,
and `UIAccessibility.isReduceTransparencyEnabled` SHALL return true.

#### Scenario: Regular glass over stripes

- **WHEN** Reduce Transparency is on and regular glass is over black and orange stripes
- **THEN** the pixels inside the glass are within 10 per channel of the Simulator's with the same setting

### Requirement: Increase Contrast makes glass stronger

When Increase Contrast is on, pomme SHALL draw all glass with the look and
border measured on the iOS 26 Simulator with that setting on, and
`UIAccessibility.isDarkerSystemColorsEnabled` SHALL return true.

#### Scenario: Tinted glass with Increase Contrast

- **WHEN** Increase Contrast is on and tinted glass is drawn
- **THEN** the pixels inside and on the border are within 10 per channel of the Simulator's

### Requirement: The settings come from the user or the host

pomme SHALL take the two settings from `pomme run` flags
(`--reduce-transparency`, `--increase-contrast`), or from the host system's
settings in the desktop window and in the browser. When a setting changes
while the app runs, pomme SHALL post the notification iOS posts and redraw.

#### Scenario: Browser setting

- **WHEN** the browser reports `prefers-reduced-transparency: reduce`
- **THEN** the app's glass is drawn frosted, and the app is told that the setting changed
