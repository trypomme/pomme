# Spec Delta

## MODIFIED Requirements

### Requirement: One host interface for every request

pomme SHALL send every URL request through one host interface. The CLI
SHALL answer with real HTTP. The browser build SHALL answer with an
asynchronous browser request (XHR). A test SHALL be able to answer with
fixtures. When there is no host interface, the phone SHALL be offline.

#### Scenario: Test with fixtures

- **WHEN** a test gives hello-expo fixtures for the GitHub repository URLs
- **THEN** the home screen shows "vercel/swr ★ 31,415" and "TanStack/query ★ 48,000", and no real request is made

#### Scenario: No network at all

- **WHEN** hello-expo runs with no host interface
- **THEN** its cards show "Couldn't load" and "Promise.all failed"

### Requirement: Answers arrive later, on the main run loop

pomme SHALL make the request when a task resumes. It SHALL deliver the
result on the main run loop, one delegate callback per run loop turn, and
not earlier than 50 ms of virtual time after the task resumed.

In a headless run, and whenever fixtures or the offline phone answer, the
answer SHALL be there at once and SHALL be delivered 50 ms later in virtual
time. A run with the same answers SHALL be the same run.

#### Scenario: SWR, TanStack Query and Promise.all

- **WHEN** hello-expo loads its three repositories from fixtures and the user taps both refresh buttons at 5 s and 5.5 s
- **THEN** the screen shows "Promise.all: 3 repos, ★ 319,415", and `facebook/react` was requested exactly once

## ADDED Requirements

### Requirement: Live front ends keep running while a request is out

In a live front end (the browser build and the desktop window) with a live
network, a request SHALL go out in the background. The app SHALL keep
running, drawing frames and taking input while the request is out. The
answer SHALL be delivered at the first main run loop turn after it is back
(and not earlier than 50 ms of virtual time after the task resumed).

#### Scenario: A tap during a slow request

- **WHEN** hello-expo runs in the browser, a request it made takes 2 seconds to answer, and the user taps the counter while it is out
- **THEN** the counter shows the tap before the request has answered

#### Scenario: Cancel while out

- **WHEN** an app cancels a task whose request is still out
- **THEN** the app gets the cancellation error, and the answer that comes back later is dropped
