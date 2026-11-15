# hosts/cli Specification

## Purpose

The `pomme` command is how a person runs an iOS app in pomme from a
terminal. It can look inside an app, load it, run it with scripted input,
and report what happened. This spec describes the commands and the options
of `pomme run`.

Words used here:

- **Virtual time**: the app's own clock inside pomme. It is not the wall
  clock. Scripted input happens at virtual times, so a run repeats exactly.
- **Sysroot**: pomme's folder of system libraries that run as guest code
  (default `vendor/sysroot`).
- **HLE surface**: the system libraries and symbols an app needs from iOS,
  which pomme must provide.
- **Stop**: the run ends early because something went wrong (a missing
  function, a CPU fault, a limit).

## Requirements

### Requirement: Commands

`pomme` SHALL have these commands: `inspect` (list every image in a `.app`
and its HLE surface; `--symbols` lists every imported symbol), `load` (map
and link the app and report the result; `--stubs` lists every bound stub),
`run` (run the app), `coverage` (see `tools/coverage`) and `bundle` (pack
the app and the sysroot files it loads into one file for the browser host).

#### Scenario: Inspect an app

- **WHEN** a user runs `pomme inspect helloexpo.app`
- **THEN** pomme lists the images in the bundle and, under "HLE surface", the number of system libraries and distinct symbols pomme must provide

### Requirement: A run has limits

`pomme run` SHALL stop when the app is idle past `--seconds` of virtual time
(default 10) or after `--steps` instructions (default 50,000,000). When it
starts, it SHALL say which iOS it presents, which SDK the app was linked
against, and which design (Liquid Glass or pre-iOS 26) the app gets.

#### Scenario: Default limit

- **WHEN** a user runs `pomme run App.app` with no limits
- **THEN** the run ends once the app has been idle for 10 s of virtual time

### Requirement: Scripted input at virtual times

`pomme run` SHALL accept input at virtual times, each option repeatable:
`--tap X,Y@T`, `--swipe X1,Y1>X2,Y2@T` (0.3 s; `~D` after the time for
another duration), `--type 'TEXT@T'` (`\n` return, `\b` delete, to the first
responder), `--paste 'TEXT@T'`, `--rotate ORIENTATION@T`, `--face match@T`.
Points are in points, not pixels.

#### Scenario: Two taps

- **WHEN** a user runs `pomme run helloexpo.app --tap 195,438@3 --tap 195,438@4 --screenshot out.png`
- **THEN** the app receives two taps, at 3 s and 4 s, and the screenshot shows "Tapped 2 times"

### Requirement: The phone's setup

`pomme run` SHALL set up the presented phone from options: `--device` (the
iPhone; `--device list` lists them), `--ios` (the iOS version), `--url` (a
deep link at launch), `--orientation`, `--appearance`, `--battery`,
`--network`, `--status-bar` (pinned for screenshots), `--biometry`,
`--clock`, `--clipboard`, `--paste-permission`, `--env KEY=VALUE`, and
launch arguments after `--`.

#### Scenario: Deep link

- **WHEN** a user runs `pomme run helloexpo.app --url 'pommehello:///keyboard'`
- **THEN** the app launches with that URL, as iOS passes one

### Requirement: Repeatable network for runs

By default `pomme run` SHALL use the real network. `--offline` SHALL make
every request fail as on a device with no network. `--fixture URL=FILE`
SHALL answer requests for URL (or a prefix ending in `*`) with the file.
With `--offline`, the phone's clock SHALL start at 2026-01-01T00:00:00Z, so
runs repeat; otherwise it SHALL start at the current time.

#### Scenario: Fixture answer

- **WHEN** a user runs `pomme run App.app --fixture 'https://api.x/*=x.json'`
- **THEN** each request to a URL that starts with `https://api.x/` gets the contents of `x.json`

### Requirement: What a run reports

When the run ends, `pomme run` SHALL print how it ended (the exit code, or
the stop reason), the number of instructions and the time taken, and the
15 system functions called most. `--views` SHALL print the view hierarchy,
`--elements` what accessibility sees, and `--screenshot FILE` SHALL write a
PNG at the device's scale (`--shell`: inside a phone body).

#### Scenario: Screenshot

- **WHEN** a user runs `pomme run App.app --screenshot out.png`
- **THEN** `out.png` has the screen at the device's scale, and pomme prints "screenshot: out.png"

### Requirement: Stops explain themselves

When the run stops, pomme SHALL print the reason, where it was called
from (for a missing function or method) or where it happened (for a CPU
fault), the thread, `pc`, `lr` and `sp`, a backtrace of up to 16 frames
with symbol names, and registers `x0` to `x7`. With `--trace` or
`POMME_THREADS` set, it SHALL print `x0` to `x27`, the other threads and the
dispatch queues.

#### Scenario: Missing function

- **WHEN** the app calls a system function pomme does not provide
- **THEN** the run stops, and pomme prints the function's name, its caller, the thread and a backtrace

### Requirement: Live run in a window

`pomme run --window` SHALL run the app live in a desktop window (see
`hosts/desktop-window`), with no step or time limit, until the window
closes.

#### Scenario: Window run

- **WHEN** a user runs `pomme run helloexpo.app --window`
- **THEN** a window shows the app and the run continues until the window is closed
