# Spec Delta

## ADDED Requirements

### Requirement: Text direction reaches Flutter

A Flutter app with a right-to-left locale SHALL lay out right-to-left.

#### Scenario: Arabic

- **WHEN** hello-flutter runs with `--language ar`
- **THEN** `Directionality.of(context)` is right-to-left
