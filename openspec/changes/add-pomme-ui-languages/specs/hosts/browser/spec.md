# Spec Delta

## ADDED Requirements

### Requirement: The page speaks the UI language

The page SHALL show its messages (loading, the first frame, the missing
JSPI or WebGPU notice, the stop message) in the UI language (see
`hosts/ui-language`). It SHALL take the UI language from the `ui` URL
setting, else from `navigator.languages`. It SHALL set the document's
`lang` attribute to the UI language. The `ui` setting SHALL NOT change the
`language` setting, which is the phone's language.

#### Scenario: Japanese page, English phone

- **WHEN** the page is opened with `?ui=ja`
- **THEN** the status messages are in Japanese, the document's `lang` is `ja`, and the app sees `en-US` as its preferred language

#### Scenario: Browser in Spanish

- **WHEN** the browser's languages are `es-ES`, then `en`, and the URL has no `ui` setting
- **THEN** the status messages are in Spanish
