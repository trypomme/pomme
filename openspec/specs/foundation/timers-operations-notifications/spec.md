# foundation/timers-operations-notifications Specification

## Purpose

Let apps run work later, on other threads, or when something happens:
timers, operation queues, notifications, dynamic calls (`NSInvocation`) and
`performSelector`. All of it runs on pomme's virtual clock and scheduler, so
the same run always does the same work in the same order.

Words used here:

- **Run loop**: the loop on a thread that waits for events and timers and
  calls the code waiting for them.
- **Virtual time**: pomme's own clock. It moves with the instructions the
  app runs, and skips ahead when every thread is waiting.
- **Operation**: a piece of work (`NSOperation`). It can depend on other
  operations and runs on a queue.
- **Type encoding**: the string that describes a method's argument types,
  for example `v24@0:8@16`.

Measured data: `crates/pomme-cli/tests/fixtures/sdkstartup/` (operation
queues, dispatch) and `fixtures/runtime/`.

## Requirements

### Requirement: Timers fire on the run loop

`NSTimer` SHALL be a CoreFoundation run loop timer. A scheduled timer SHALL
call its target and selector, or its block, when its fire date comes on
the virtual clock, again after each interval if it repeats, and never after
`invalidate`. `fire`, `fireDate` and `setFireDate:` SHALL work as in
Foundation.

#### Scenario: A repeating timer

- **WHEN** an app schedules a repeating timer every 1 s and the virtual clock moves 3 s
- **THEN** the timer has fired 3 times

#### Scenario: Invalidated

- **WHEN** the app invalidates the timer
- **THEN** it does not fire again

### Requirement: Operations run when they are ready

An operation SHALL be ready when its dependencies have finished (a
cancelled one at once). Its default `start` SHALL run `main` between the
executing and finished states. An asynchronous operation SHALL finish when
it says so through KVO on `isFinished`. Then its completion block SHALL run
on a background thread.

#### Scenario: A dependency

- **WHEN** operation B depends on operation A and both are added to a queue
- **THEN** B starts only after A has finished

### Requirement: Queues run operations on threads

A queue SHALL start its ready operations, highest priority first, up to
`maxConcurrentOperationCount` at once, each on its own thread, unless it is
suspended. The main queue SHALL run them on the main thread. A queue with
an `underlyingQueue` SHALL run them on that dispatch queue.

#### Scenario: An underlying dispatch queue

- **WHEN** an app sets a serial dispatch queue as an operation queue's `underlyingQueue`
- **THEN** `underlyingQueue` returns it, and the queue's operations run on it

### Requirement: Notifications reach their observers

`NSNotificationCenter.defaultCenter` SHALL call every observer that matches
a posted notification's name and object: target-and-selector observers and
block observers (which get a token). `removeObserver:` SHALL stop them.

#### Scenario: Block observer

- **WHEN** an app adds a block observer for a name and posts that name with `userInfo`
- **THEN** the block runs with the notification, and after `removeObserver:` with its token it no longer runs

### Requirement: Dynamic calls follow the arm64 calling rules

`NSMethodSignature` SHALL be parsed from a type encoding. `NSInvocation`
SHALL hold arguments and, on `invoke`, pass them as Apple's arm64 calling
convention does: integers and pointers in x registers, floats and
geometry structs in v registers, big structs by reference, the rest on the
stack. Big struct results SHALL use x8.

#### Scenario: A CGRect argument

- **WHEN** an app invokes a method that takes a `CGRect` through `NSInvocation`
- **THEN** the method receives the same rectangle as with a direct call

### Requirement: performSelector, now, later and on other threads

`performSelector:` (with up to two objects) SHALL call the method at once.
`performSelector:withObject:afterDelay:` SHALL call it on the current run
loop after the delay. `performSelectorOnMainThread:` and
`performSelector:onThread:` SHALL run it on that thread, and SHALL wait for
it when `waitUntilDone` is YES.

#### Scenario: After a delay

- **WHEN** an app calls `performSelector:withObject:afterDelay:` with 0.5 s
- **THEN** the method runs once, 0.5 s later on the virtual clock, on the main run loop
