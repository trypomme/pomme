# core/threads-and-time Specification

## Purpose

Run all of the app's threads and give them a clock, in a way that is the
same on every run. Many apps use several threads (React Native's JavaScript
thread, Hermes's garbage collector, dispatch workers), and all of them need
time, timers and run loops.

Words used here:

- **Virtual time**: the phone's time inside pomme. It comes from the number
  of instructions run, not from the host's clock.
- **Mach tick**: the unit of `mach_absolute_time`.
- **Quantum**: how many instructions a thread may run before another
  runnable thread gets a turn.
- **Blocking**: a thread stops and waits for something (a message, a lock, a
  timer, another thread).
- **Run loop**: the loop a thread runs to wait for events and timers
  (CFRunLoop, NSRunLoop). It sleeps on Mach ports.
- **Idle**: every thread is waiting and no timer is due.

## Requirements

### Requirement: Instructions are the clock

One instruction SHALL be 1 ns of virtual time (a 1 GHz phone). Mach ticks
SHALL be 24 MHz: `mach_timebase_info` SHALL give 125/3. Time read by the app
(`mach_absolute_time`, `clock_gettime`, `gettimeofday`, `NSDate`) SHALL come
from this clock.

#### Scenario: Timebase

- **WHEN** the app calls `mach_timebase_info`
- **THEN** it gets numer 125 and denom 3

### Requirement: The wall clock at boot

The phone's wall clock SHALL start at the boot time and move with virtual
time. With `--offline` the boot time SHALL be 2026-01-01T00:00:00Z, the same
on every run. With a live network it SHALL be the host's time. `--clock`
SHALL set it to `now`, a UTC date, or seconds since 1970. The time zone
SHALL be UTC.

#### Scenario: Fixed clock

- **WHEN** a developer runs `pomme run <app> --clock 2026-03-14T09:30:00Z`
- **THEN** the app's `Date()` at launch is 2026-03-14 09:30:00 UTC

### Requirement: Guest threads on one host thread

pomme SHALL give each guest thread its own registers, stack, TLS area and
thread-specific data, and SHALL run only one at a time, on one host thread.
A thread SHALL switch when it blocks (a Mach message receive, a semaphore,
a condition variable, a lock, a join, a sleep), or after a quantum of
2,000,000 instructions. A run SHALL be the same instruction for instruction.

#### Scenario: Three threads in hello-expo

- **WHEN** hello-expo starts
- **THEN** the main thread, React Native's JavaScript thread and Hermes's garbage collector thread all run, and two runs give the same screen at the same virtual time

### Requirement: Time skips when idle

When every thread waits, virtual time SHALL jump to the earliest deadline
(a timer or a timeout). When there is no deadline, the app is idle. `pomme
run` SHALL stop once the app is idle past `--seconds` of virtual time
(default 10).

#### Scenario: A 5-second timer

- **WHEN** the only thing left is an `NSTimer` that fires in 5 seconds
- **THEN** virtual time jumps to it and the timer fires, without running 5 billion instructions

### Requirement: Run loops, Mach ports and timers

pomme SHALL provide Mach ports and messages, and Mach timers that send a
message to their port when due, so that CoreFoundation's real run loops
sleep and wake. `NSRunLoop`, `performSelector:` on another thread or after a
delay, and `CADisplayLink` (60 frames per second) SHALL work on top of them.

#### Scenario: Display link

- **WHEN** the app adds a `CADisplayLink` to the main run loop
- **THEN** its selector is called once every 1/60 s of virtual time

### Requirement: Dispatch

`dispatch_async` and related calls SHALL run their work on the queue's
thread: the main queue on the main run loop, in order, and other queues on
dispatch worker threads, which wait for more work when they are done.
Dispatch sources SHALL start inactive, coalesce events, and run their
handlers on their queue; a timer source SHALL fire on the virtual clock.
Sources on descriptors, signals, processes and Mach ports SHALL stop the run
with a message.

#### Scenario: Timer source

- **WHEN** the runtime fixture resumes a dispatch timer source
- **THEN** its handler runs on its queue at each interval, and the fixture reports `sources=1`

### Requirement: Locks behave as on Darwin

Mutexes (normal, recursive, error-checking), read-write locks,
`os_unfair_lock`, `OSSpinLock`, `pthread_once` and `dispatch_once` SHALL keep
other threads out until released. A thread that cannot take a lock SHALL
block. An error-checking or recursive mutex SHALL report EDEADLK or EPERM
where Darwin does. A once-initializer SHALL run exactly once.

#### Scenario: Atomics and threads

- **WHEN** the runtime fixture counts with several threads under a lock and with atomics
- **THEN** the totals are exact and it reports `atomics=1` and `threads=1`

### Requirement: Say why a run stopped waiting

When the app can make no progress, pomme SHALL stop and describe what the
waiting thread waits for. When a deadline is past the run's time limit, it
SHALL stop and say how many seconds of virtual time were reached.

#### Scenario: Time limit

- **WHEN** the app is idle past `--seconds 2`
- **THEN** pomme stops with "reached the time limit (2.0 s of virtual time), idle"
