# foundation/formats-and-parsing Specification

## Purpose

Read and write the data formats apps and SDKs use (JSON, property lists,
keyed archives), and give them Foundation's helpers for dates, numbers,
text patterns and identifiers. The output must be byte-for-byte what iOS
writes where apps depend on it, so that saved data and network payloads
look the same.

Words used here:

- **Property list (plist)**: Apple's file format for dictionaries, arrays,
  strings, numbers, dates and data. It can be XML, binary or OpenStep text.
- **Keyed archive**: Apple's format for saving a graph of objects
  (`NSKeyedArchiver`). It is a binary plist with `$archiver`, `$version`,
  `$top` and `$objects`.
- **ICU**: the library iOS uses for dates, numbers and calendars in every
  language. pomme runs ICU from source, inside CoreFoundation.
- **Device settings**: the phone is set to English (United States),
  `en_US`, with the GMT time zone, like a fresh Simulator.

Measured data: `crates/pomme-cli/tests/fixtures/sdkstartup/` (formatters,
calendar, plists, archives, scanner, UUID, character sets) and
`fixtures/charsets/`.

## Requirements

### Requirement: JSON is read and written like Foundation

`NSJSONSerialization` SHALL turn strings, numbers, booleans, `NSNull`,
arrays and dictionaries into JSON and back. Writing SHALL escape `/` unless
the app asks not to, SHALL write `3.0` as `3`, and SHALL support sorted keys
and pretty printing (two spaces).

#### Scenario: Escaped slash

- **WHEN** an app writes `{"url": "https://a/b"}` with no options
- **THEN** the JSON is `{"url":"https:\/\/a\/b"}`

#### Scenario: Sorted keys without escaping

- **WHEN** an app writes the same object with sorted keys and without escaping slashes
- **THEN** the keys come out in order and the URL is `https://a/b`

### Requirement: Property lists are read and written

pomme SHALL read XML, binary and OpenStep plists, and write XML and binary
ones, with Foundation's mutability options. `dictionaryWithContentsOfFile:`
and `arrayWithContentsOfFile:` SHALL give nil when the file is missing, is
not a plist, or holds another class. Bad data SHALL fail with error code
3840.

#### Scenario: Round trip through a file

- **WHEN** an app writes a dictionary to `config.plist` with `writeToFile:atomically:` and reads it back
- **THEN** the dictionary read is equal to the one written

#### Scenario: Wrong class

- **WHEN** the file holds a dictionary and the app reads it with `arrayWithContentsOfFile:`
- **THEN** the result is nil

### Requirement: Keyed archives use Apple's format

`NSKeyedArchiver` SHALL write Apple's binary keyed archive format, with
Foundation's classes stored as Foundation stores them, and other objects
through their `encodeWithCoder:`. `NSKeyedUnarchiver` SHALL read it back
through `initWithCoder:`. Data that is not an archive SHALL give nil.

#### Scenario: An app's own class

- **WHEN** an app archives an object of its own class (a title, a count, a score, a flag and tags) and unarchives it
- **THEN** every value comes back equal, and the archive's `$archiver` is `NSKeyedArchiver` and `$objects[0]` is `$null`

### Requirement: Number and date formatters use ICU

`NSNumberFormatter` and `NSDateFormatter` SHALL format and parse with ICU,
with the app's style, format, locale and time zone. Without a locale they
SHALL use `en_US`; without a time zone, GMT. A string that does not match
SHALL parse to nil.

#### Scenario: Decimal and percent

- **WHEN** an `en_US` decimal formatter formats 1234567.5, and a percent formatter formats 0.25
- **THEN** the results are `1,234,567.5` and `25%`

#### Scenario: A named time zone

- **WHEN** a formatter with format `HH:mm` and time zone `Australia/Sydney` formats 2026-01-01 01:01:01 GMT
- **THEN** the result is `12:01` (Sydney is 11 hours ahead in summer)

### Requirement: Calendars do Gregorian arithmetic

`NSCalendar` SHALL be Gregorian, with weeks starting on Sunday. It SHALL
split dates into components in its time zone, put components together
leniently, and add months keeping the day where it can. Time zones SHALL be
fixed offsets or IANA zones with their daylight saving rules.

#### Scenario: One month after 31 January

- **WHEN** an app adds one month to 31 January 2026, 09:00
- **THEN** the result is 28 February 2026, 09:00, a Saturday (weekday 7)

### Requirement: Locale is English (United States)

`NSLocale.currentLocale` SHALL be `en_US`. Locale identifiers such as
`en-US`, `zh-Hans-CN` and `en_US_POSIX` SHALL be split into language,
script, region and variant.

#### Scenario: Current locale

- **WHEN** an app reads `NSLocale.currentLocale.localeIdentifier`
- **THEN** it is `en_US`

### Requirement: Regular expressions

`NSRegularExpression` SHALL match the common pattern subset (classes,
groups, quantifiers, anchors, `\b \d \s \w`, case-insensitive) as ICU does,
with ranges in UTF-16 units. A pattern with lookaround or backreferences
SHALL fail to compile, like an invalid pattern.

#### Scenario: Hex color

- **WHEN** an app searches `"ffa500FF"` for a hex color pattern, as Expo parses colors
- **THEN** the match starts at 0 and covers `ffa500FF`

### Requirement: Scanners read strings piece by piece

`NSScanner` SHALL scan literal strings, runs of a character set and numbers,
skipping whitespace and newlines first unless told otherwise. A scan that
matches nothing SHALL return NO and leave the position unchanged.

#### Scenario: Firebase app ID

- **WHEN** an app scans `"1:1234:ios:ab12"` as digits, `:`, a number, `:`, text up to `:`, `:`, and a hex number
- **THEN** it reads `1`, 1234, `ios` and 0xab12, and the scanner is at the end

#### Scenario: A scan that fails

- **WHEN** an app scans a double from `"  -2.5e1 x"` and then an integer
- **THEN** the double is -25, the integer scan returns NO, and the position stays at 8

### Requirement: UUIDs are random but repeat between runs

`NSUUID` SHALL make version 4 UUIDs from pomme's seeded random generator, so
the same run makes the same UUIDs. It SHALL parse and write the
`E621E1F8-C36C-495A-93FC-0C247A3E6E5F` form.

#### Scenario: Two new UUIDs

- **WHEN** an app makes two UUIDs in one run
- **THEN** they are different, each string is 36 characters long, and the next run makes the same two

### Requirement: Character sets

The standard `NSCharacterSet`s SHALL be CoreFoundation's. Factory methods
sent to `NSMutableCharacterSet` SHALL give a new set the app can change,
never the shared one. Strings SHALL trim, search, split and percent-encode
with sets.

#### Scenario: A mutable copy of a standard set

- **WHEN** an app gets `[NSMutableCharacterSet alphanumericCharacterSet]` and adds characters to it
- **THEN** the new set has them, and the shared alphanumeric set does not

### Requirement: Uniform type identifiers

The UTType functions SHALL know the system's common types with their file
extensions, MIME types and the types they conform to. A tag no type claims
SHALL get a dynamic identifier.

#### Scenario: PNG from its extension

- **WHEN** an app asks for the type of the extension `png`
- **THEN** it gets `public.png`, which conforms to `public.image`
