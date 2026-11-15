# tools/coverage Specification

## Purpose

`pomme coverage <app>` says how much of iOS an app needs, how much of it
pomme provides, and what a run of the app reached that pomme does not
provide. It tells developers what to build next.

Words used here:

- **Bound symbol**: a system function or data symbol the app imports, which
  the loader connected to pomme.
- **Miss**: a call to a missing function or Objective-C method.
- **Permissive class**: a class that answers messages it does not know with
  a generic answer, instead of stopping. This is a place where pomme may
  behave differently from iOS without stopping.

Functions that come from the sysroot's guest code (the Swift runtime,
libc++, CoreFoundation) are not counted.

## Requirements

### Requirement: Every bound symbol is classified

The report SHALL classify every system symbol the app binds to pomme as
**implemented** (pomme handles calls to it), **data** (pomme backs it with
real memory: a class, a constant) or **missing** (using it stops the app),
grouped by library, with how many implemented ones the run called.

#### Scenario: A UIKit app

- **WHEN** coverage runs on the coverage fixture
- **THEN** `_objc_msgSend` is implemented and was called, `_OBJC_CLASS_$_UIView` is data, and `_pomme_coverage_probe` is missing

### Requirement: One run finds every miss

During the run, a miss SHALL answer 0 (nil) and the app SHALL carry on, so
one run finds all misses. Each missing function and method SHALL be listed
with its count and where it was first called from. The run's first miss
SHALL be marked, because everything after it ran on a stand-in answer.

#### Scenario: Two calls to a missing function

- **WHEN** the coverage fixture calls the missing probe function twice and a missing method once
- **THEN** the report lists the function with count 2, marked first, called first from `viewDidLoad`, and the method `-[NSObject pommeCoverageProbe]` with count 1

### Requirement: Generic answers are split three ways

Messages that permissive classes answered generically SHALL be listed in
three groups: setters whose value nothing in pomme reads (no effect, unlike
iOS: the real gaps), setters whose value pomme reads (in effect
implemented), and other generic answers.

#### Scenario: A setter nothing reads

- **WHEN** the app calls `-[UIView setPommeCoverageProbe:]`
- **THEN** it is listed as stored but unused, and a text field's `setText:` is not

### Requirement: Options

`coverage` SHALL take every `run` option. `--no-run` SHALL stop where dyld
hands over to `main`. `--stop-at-first` SHALL stop at the first miss, as
`run` does. `--all` SHALL list every missing symbol (by default the first
8 per library). `--json FILE` SHALL also write the complete report as JSON.

#### Scenario: Stop at the first gap

- **WHEN** coverage runs with `--stop-at-first`
- **THEN** the run stops at the first missing function, and the report lists exactly one missing function
