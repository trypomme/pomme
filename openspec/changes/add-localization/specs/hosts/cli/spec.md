# Spec Delta

## MODIFIED Requirements

### Requirement: The phone's setup

`pomme run` SHALL set up the presented phone from options: `--device` (the
iPhone; `--device list` lists them), `--ios` (the iOS version), `--url` (a
deep link at launch), `--orientation`, `--appearance`, `--battery`,
`--network`, `--status-bar` (pinned for screenshots), `--biometry`,
`--clock`, `--clipboard`, `--paste-permission`, `--language` (repeatable;
the first is the preferred language), `--region`, `--time-zone`,
`--24-hour`, `--env KEY=VALUE`, and launch arguments after `--`.

#### Scenario: Deep link

- **WHEN** a user runs `pomme run helloexpo.app --url 'pommehello:///keyboard'`
- **THEN** the app launches with that URL, as iOS passes one

#### Scenario: Language and region

- **WHEN** a user runs `pomme run helloexpo.app --language ja-JP --language en-US --region JP`
- **THEN** the app sees `["ja-JP", "en-US"]` as its preferred languages and `JP` as its region
