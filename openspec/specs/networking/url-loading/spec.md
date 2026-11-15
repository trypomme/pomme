# networking/url-loading Specification

## Purpose

Let apps load data over HTTP with Foundation's URL loading API
(`NSURLRequest`, `NSURLSession`, `NSHTTPURLResponse`), while keeping every
run repeatable. All requests go through one host interface, so the front
end decides where the answers come from.

Words used here:

- **Host interface**: the one place where pomme asks the machine it runs on
  for something from outside the phone (here: HTTP answers).
- **Front end**: the program that runs pomme: the CLI, the browser build, or
  a test.
- **Fixture**: a saved answer for a URL, used instead of the real network.
- **Virtual time**: pomme's own clock. It moves with the app's instructions,
  not with the wall clock.
- **Run loop**: the main thread's loop that delivers events and callbacks.

## Requirements

### Requirement: One host interface for every request

pomme SHALL send every URL request through one host interface. The CLI
SHALL answer with real HTTP. The browser build SHALL answer with a
synchronous browser request (XHR). A test SHALL be able to answer with
fixtures. When there is no host interface, the phone SHALL be offline.

#### Scenario: Test with fixtures

- **WHEN** a test gives hello-expo fixtures for the GitHub repository URLs
- **THEN** the home screen shows "vercel/swr ★ 31,415" and "TanStack/query ★ 48,000", and no real request is made

#### Scenario: No network at all

- **WHEN** hello-expo runs with no host interface
- **THEN** its cards show "Couldn't load" and "Promise.all failed"

### Requirement: CLI network options

`pomme run` SHALL accept `--offline` (every request fails as on an offline
phone) and `--fixture URL=FILE` (answer that URL with that file; a URL
ending in `*` is a prefix). Requests that no fixture answers SHALL go to
real HTTP. `--log-requests` SHALL print each request with its status and
size.

#### Scenario: A prefix fixture

- **WHEN** the user runs `pomme run <app> --fixture 'https://api.x/*=x.json'`
- **THEN** a request for `https://api.x/items?page=2` is answered with the bytes of `x.json`

#### Scenario: Offline flag

- **WHEN** the user runs `pomme run <app> --offline`
- **THEN** every request fails with an error, as on a phone with no connection

### Requirement: Fixtures answer by URL

A fixture SHALL match a request whose URL is the same, or starts with the
fixture's URL without its final `*`. A fixture SHALL be able to carry a
file's bytes with its content type. A request that no fixture matches SHALL
get status 404 with the body "not found".

#### Scenario: Image fixture

- **WHEN** a test serves `tests/fixtures/wide.jpg` as `image/jpeg` for `https://images.pomme.test/wide.jpg`
- **THEN** expo-image on hello-expo's /image screen reports the remote picture as 300x100

### Requirement: Session tasks behave like Foundation's

pomme SHALL support data tasks, download tasks (the body saved as a
temporary file) and upload tasks (from data or from a file), with a
completion handler or with the session delegate. A delegate SHALL hear the
response, then the data, then completion, as Foundation sends them.
Response header names SHALL be in Foundation's canonical form, so
`Content-Type` is found even when the server sent it in lower case.

#### Scenario: Download task

- **WHEN** an app starts a download task with a completion handler
- **THEN** the handler gets a file URL whose file holds the response body

#### Scenario: Lower-case headers

- **WHEN** an HTTP/2 server answers with the header `content-type`
- **THEN** `[response valueForHTTPHeaderField:@"Content-Type"]` returns its value

### Requirement: Answers arrive later, on the main run loop

pomme SHALL make the request when a task resumes, and SHALL deliver the
result on the main run loop 50 ms later in virtual time, one delegate
callback per run loop turn. A run with the same answers SHALL be the same
run.

#### Scenario: SWR, TanStack Query and Promise.all

- **WHEN** hello-expo loads its three repositories from fixtures and the user taps both refresh buttons at 5 s and 5.5 s
- **THEN** the screen shows "Promise.all: 3 repos, ★ 319,415", and `facebook/react` was requested exactly once

### Requirement: The modeled network can make requests fail

When the modeled network path is offline (`--network offline`, or a change
to offline while the app runs), pomme SHALL fail requests, even when a host
interface or fixtures could answer them.

#### Scenario: Offline path with fixtures

- **WHEN** hello-expo has fixtures for its repositories and the network path is offline
- **THEN** its cards show "Couldn't load"

### Requirement: The phone's clock fits the network

The phone's clock at launch SHALL be the real time when the network is
live, because TLS certificates and sign-in tokens are checked against it.
With `--offline` it SHALL be 2026-01-01T00:00:00Z on every run. `--clock`
SHALL set it to `now`, a date, or seconds since 1970.

#### Scenario: Offline clock

- **WHEN** an app runs with `--offline` and reads `NSDate.date` at launch
- **THEN** the date is 2026-01-01 00:00:00 UTC
