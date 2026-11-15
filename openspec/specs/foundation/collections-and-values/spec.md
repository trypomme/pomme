# foundation/collections-and-values Specification

## Purpose

Give apps Foundation's basic value types: arrays, dictionaries, sets,
strings, numbers and boxed values. They must compare, hash, change and
format exactly as on iOS, because apps and SDKs make decisions from them
(React Native compares dictionaries to decide if the window changed).

Words used here:

- **Compare by content**: two collections are equal (`isEqual:`) when they
  hold equal objects, even if they are different objects in memory.
- **Toll-free bridging**: an `NSArray` can be used as a `CFArrayRef` (and
  the same for dictionaries, strings, data, numbers) with no conversion.
- **Literal**: an object the compiler writes into the app, for example
  `@"text"`, `@[@1, @2]` or `@{@"k": @1}`.

Measured data: `crates/pomme-cli/tests/fixtures/collections/`,
`fixtures/sdkstartup/`, `fixtures/runtime/`, `fixtures/stringformat/` and
`fixtures/stringsearch/`.

## Requirements

### Requirement: Collections compare by content

`NSDictionary`, `NSArray` and `NSSet` SHALL be equal when their contents are
equal, and equal collections SHALL have the same `hash`. A mutable copy SHALL
be equal to its original. A collection SHALL NOT be equal to an object of
another kind.

#### Scenario: Nested dictionaries

- **WHEN** two dictionaries hold the same nested dictionary, one made with `@(390.0)` and one with `numberWithDouble:390.0`
- **THEN** they are `isEqual:`, have the same `hash`, and a set with one of them contains the other

#### Scenario: Sets in a different order

- **WHEN** one set is made from `"a"` and `2`, and another from `2` and `"a"`
- **THEN** the two sets are equal and have the same hash

### Requirement: Collections can be changed in bulk

Mutable arrays and dictionaries SHALL support Foundation's bulk edits:
removing a range, removing at an index set, exchanging, replacing a range
with an array, and removing the objects in an array. Set algebra (subset,
intersects, union, minus) SHALL give Foundation's answers.

#### Scenario: Bulk edits on an array

- **WHEN** an app takes `[0 1 2 3 4 5]`, removes range (1, 2), exchanges index 0 and 3, removes index 1, replaces range (0, 1) with `[7 8]`, and removes `4`
- **THEN** the array is `[7 8 0]`

### Requirement: Ordered sets, index sets and pointer arrays

`NSOrderedSet` SHALL keep distinct objects in the order they were added.
`NSIndexSet` SHALL keep sorted ranges of indexes and answer queries and
enumeration. `NSPointerArray` SHALL hold pointers, NULL included, with weak
or strong object entries.

#### Scenario: Ordered set

- **WHEN** an app makes a mutable ordered set from `["b", "a", "b", "c"]`
- **THEN** it holds `b`, `a`, `c`, in that order, once each

### Requirement: Subclasses of collections and strings work everywhere

An app's own subclass of `NSArray`, `NSDictionary` or `NSString` (and
Swift's bridged arrays, dictionaries and strings) SHALL work with
Foundation's methods and with CoreFoundation functions, through the
subclass's primitive methods. `NSNull` SHALL have one instance, equal to
`kCFNull`.

#### Scenario: A dictionary subclass

- **WHEN** an app wraps a dictionary in its own `NSDictionary` subclass and uses `for-in`, `allKeys` and `valueForKey:` on it
- **THEN** each gives the wrapped dictionary's contents

#### Scenario: CoreFoundation on a subclass

- **WHEN** C code calls `CFArrayGetCount` and `CFGetTypeID` on an app's `NSArray` subclass with three objects
- **THEN** the count is 3 and the type is `CFArrayGetTypeID()`

### Requirement: Strings search, compare and change like Foundation

`NSString` SHALL support search with options (case-insensitive, backwards,
anchored, literal, regular expression), comparison (case-insensitive,
localized standard order), replacing (with templates and empty matches),
paths from components, and percent encoding. Initializers sent to
`NSMutableString` SHALL make mutable strings.

#### Scenario: Search options

- **WHEN** the stringsearch probe runs its searches
- **THEN** each result matches the expected line in `stringsearch` (for example `FOUND caseless 2:ll`, `FOUND anchored none`, `FOUND template Smith, John`)

#### Scenario: Natural sort order

- **WHEN** an app compares `"file10"` with `"file9"` using `localizedStandardCompare:`
- **THEN** the answer is "descending" (`file10` comes after `file9`)

### Requirement: String(format:) takes every kind of argument

Format strings (`String(format:)`, `stringWithFormat:`) SHALL accept Int,
Double, CGFloat, Float, String, NSString, Int64 and UInt arguments, and
format them as iOS does.

#### Scenario: Mixed arguments

- **WHEN** the stringformat probe formats `"%@ is %d (%.1f%%)"` with `x`, 3 and 42.5
- **THEN** the result is `x is 3 (42.5%)`

### Requirement: Numbers and boxed values

`NSNumber` SHALL compare and convert between integer, floating point and
boolean values as Foundation does. `NSValue` SHALL box pointers, objects
that are not retained, geometry structs (`CGRect`, `CGPoint`, `CGSize`, ...)
and any value given with its type encoding, and give them back unchanged.

#### Scenario: Boxed rectangle

- **WHEN** an app boxes `CGRectMake(1, 2, 3, 4)` in an `NSValue` and reads it back
- **THEN** it gets `(1, 2, 3, 4)`

### Requirement: Foundation classes adopt their protocols

Foundation's classes SHALL conform to their standard protocols
(`NSCopying`, `NSMutableCopying`, `NSSecureCoding`, `NSFastEnumeration`) when
the runtime is asked, because Swift casts such as `as! NSCopying` ask.

#### Scenario: Swift asks about NSCopying

- **WHEN** code asks if a string conforms to `NSCopying`, and if `NSDictionary` conforms to `NSSecureCoding`
- **THEN** both answers are yes
