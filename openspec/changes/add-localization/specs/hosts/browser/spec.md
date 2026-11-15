# Spec Delta

## ADDED Requirements

### Requirement: Language, region and time zone settings

The page SHALL take the preferred languages, the region and the time zone
from URL settings (`language`, `region`, `timeZone`). It SHALL offer a
choice to use the browser's own languages (`navigator.languages`) and time
zone. Without a setting or that choice, it SHALL use pomme's defaults, so a
run looks the same in every browser.

#### Scenario: Japanese from the URL

- **WHEN** the page is opened with `?language=ja-JP&region=JP`
- **THEN** the app sees `ja-JP` first in `preferredLanguages` and `JP` as its region

### Requirement: Fonts for other scripts load only when needed

A bundle SHALL include a fallback font for a script only when the app has a
localization in that script or the run asks for that language.

#### Scenario: An English-only app

- **WHEN** `pomme bundle` packs an app that has only `en.lproj`
- **THEN** the bundle has no Japanese, Chinese, Korean, Arabic, Hebrew or Thai font
