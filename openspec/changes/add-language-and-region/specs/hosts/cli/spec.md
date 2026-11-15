# Spec Delta

## ADDED Requirements

### Requirement: Language and region options

`pomme run` SHALL set up the phone's language and region from options:
`--language` (repeatable; the first is the preferred language), `--region`,
`--time-zone`, and `--24-hour` (on or off).

#### Scenario: Language and region

- **WHEN** a user runs `pomme run helloexpo.app --language ja-JP --language en-US --region JP`
- **THEN** the app sees `["ja-JP", "en-US"]` as its preferred languages and `JP` as its region
