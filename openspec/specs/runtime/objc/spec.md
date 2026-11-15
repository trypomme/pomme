# runtime/objc Specification

## Purpose

Provide the Objective-C runtime that every iOS app, and every framework
written in Objective-C or Swift, depends on: classes, message sends,
reflection and memory management.

Words used here:

- **Class metadata**: the description of a class that the compiler puts in
  the binary (`__objc_*` sections): its methods, properties and superclass.
- **System class**: a class that lives in iOS, like `NSObject` or `UIView`.
  It is not in any loaded image, so pomme makes ("synthesizes") it.
- **Message send**: a call like `[view setNeedsLayout]`, done by
  `objc_msgSend`.
- **Selector (SEL)**: the name of a method, as a value.
- **Toll-free bridging**: a CoreFoundation object and a Foundation object
  can be used as each other (`CFStringRef` and `NSString *`).
- **Permissive class**: a system class that pomme does not fully implement,
  whose unknown messages get a simple stand-in answer.

## Requirements

### Requirement: Classes from the app

pomme SHALL read the classes, categories and protocols compiled into the app
and its bundled frameworks from their metadata in guest memory, and send
messages to them. Their methods SHALL run as guest code.

#### Scenario: App class

- **WHEN** the app defines `AppDelegate` with `application:didFinishLaunchingWithOptions:`
- **THEN** UIKit's launch sends that message and the app's code runs

### Requirement: System classes are synthesized

For each system class the app refers to, pomme SHALL make a real class in
guest memory and point the app's references at it. App classes SHALL be able
to subclass system classes. System methods SHALL be pomme's own functions.

#### Scenario: Subclass of UIView

- **WHEN** an app class subclasses `UIView` and overrides `layoutSubviews`
- **THEN** pomme calls the app's override when it lays the view out

### Requirement: Message sends

`objc_msgSend`, `objc_msgSendSuper` and `objc_msgSendSuper2` SHALL find the
method on the receiver's class or its superclasses. A message to nil SHALL
return 0. Selectors SHALL be canonical, so two selectors with the same name
are the same pointer. An unknown method SHALL go through
`forwardingTargetForSelector:`, then `forwardInvocation:`, and else stop the
run naming the class and selector.

#### Scenario: Unknown method

- **WHEN** the app sends a message that no class in the chain implements and nothing forwards
- **THEN** pomme stops with "needs HLE: -[Class selector]" (`+` for a class method)

### Requirement: Reflection and changing classes

pomme SHALL support the runtime functions apps and libraries use to look at
and change classes: methods and their lists, adding methods, exchanging
implementations, associated objects, protocols and conformance, making new
classes at run time (`objc_allocateClassPair`), and class lookup by name.

#### Scenario: Method swizzling

- **WHEN** a library exchanges the implementations of two methods with `method_exchangeImplementations`
- **THEN** sending the first selector runs the second implementation

### Requirement: Reference counting

Retain and release SHALL do nothing for Objective-C and CoreFoundation
objects (pomme never frees them). For native Swift objects they SHALL
forward to the Swift runtime, so the counts are exact. Heap blocks SHALL be
reference counted too.

#### Scenario: Swift copy on write

- **WHEN** Swift code checks whether an array's buffer is uniquely referenced after Objective-C retained and released it
- **THEN** the count is exact and the check gives the right answer

### Requirement: Toll-free bridging

Foundation's collection, string, number and data classes SHALL be bridged to
CoreFoundation. An Objective-C or Swift subclass of `NSArray`,
`NSDictionary`, `NSString` or `NSData` passed to a CoreFoundation function
SHALL work: `CFGetTypeID` SHALL give the bridged type, and the reading
functions SHALL use the object's own methods. `NSNull` SHALL have one
instance, which is `kCFNull`.

#### Scenario: Swift array to CF

- **WHEN** the runtime fixture passes a Swift-bridged array to `CFArrayGetCount`
- **THEN** it gets the array's count, and the fixture reports `tollFree=1`

### Requirement: Permissive classes

A small list of configuration-style system classes (for example `NSCache`,
`CATransaction`, `CAAnimation`, `MTLCaptureManager`) SHALL answer messages
pomme does not implement: a setter is accepted and stored, other messages
return nil, 0 or NO, and a class method that returns an instance returns a
new one. `--trace` SHALL show every message answered this way.

#### Scenario: NSCache setter

- **WHEN** the app calls `setCountLimit:` on an `NSCache`
- **THEN** the call returns and the app keeps running

### Requirement: Hooking guest functions

pomme SHALL be able to replace a function in a loaded image with its own
code and still call the original. It SHALL refuse a function that is not in
an image's code or that starts with a PC-relative load, with a message.

#### Scenario: C++ throws are reported

- **WHEN** an app with the sysroot's libc++ starts
- **THEN** `__cxa_throw` is hooked so each throw is reported before it unwinds
