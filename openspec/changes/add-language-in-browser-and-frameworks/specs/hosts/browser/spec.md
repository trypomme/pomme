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
