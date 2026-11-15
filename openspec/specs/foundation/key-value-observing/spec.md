# foundation/key-value-observing Specification

## Purpose

Tell objects when a value they watch changes: key-value observing (KVO), and
iOS 17's trait change registrations for views and view controllers.
Libraries use these to follow a view's size (React Native's screens and
Expo's bottom accessory) and to know when an operation finished.

Words used here:

- **Observer**: the object that wants to hear about changes.
- **Key path**: the name of the watched value, for example `frame`.
- **Change dictionary**: what the observer receives: the kind of change
  and, if it asked, the new and the old value.
- **Trait**: a fact about the screen a view is on, such as light or dark
  mode, or the size class.

## Requirements

### Requirement: Observers register and unregister

`addObserver:forKeyPath:options:context:` SHALL register an observer for a
key path on an object, with its options and context.
`removeObserver:forKeyPath:` (and the variant with a context) SHALL remove
it. A removed observer SHALL NOT hear more changes.

#### Scenario: Remove an observer

- **WHEN** an observer is removed and the value changes
- **THEN** the observer is not called

### Requirement: The initial value

When the options include `NSKeyValueObservingOptionInitial`, the observer
SHALL hear `observeValueForKeyPath:ofObject:change:context:` at once, while
it registers.

#### Scenario: Ask for the initial value

- **WHEN** an observer registers for `hidden` on a view with the initial option
- **THEN** it is called once at once, before any change

### Requirement: Objects announce their changes

When an object calls `willChangeValueForKey:` and then
`didChangeValueForKey:`, its observers for that key SHALL be called with a
change of kind "setting", and with the new and old values when they asked
for them.

#### Scenario: An asynchronous operation finishes

- **WHEN** an `NSOperation` subclass calls `willChangeValueForKey:@"isFinished"` and `didChangeValueForKey:@"isFinished"`
- **THEN** its observers (its queue among them) hear that `isFinished` changed

### Requirement: Views announce their geometry and visibility

A view SHALL announce changes to `frame`, `center`, `bounds` and `hidden`
to its observers, with the old and new values.

#### Scenario: A view grows

- **WHEN** an observer watches a view's `bounds` with the new and old options, and the view's height changes from 40 to 80
- **THEN** the observer is called once, and the change dictionary holds both rectangles

### Requirement: Trait change registrations

`registerForTraitChanges:withHandler:`, `…withTarget:action:` and
`…withAction:` on views and view controllers SHALL call the handler, with
the object and the traits before, when one of the named traits changes:
the appearance (light or dark) or a size class (the phone turned).
`unregisterForTraitChanges:` with the returned token SHALL stop it.

#### Scenario: Dark mode turns on

- **WHEN** a view registers for `UITraitUserInterfaceStyle` and the system turns dark
- **THEN** the handler runs once with the view and the previous (light) traits
