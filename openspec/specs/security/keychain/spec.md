# security/keychain Specification

## Purpose

Store secrets for the app with the Keychain API (`SecItemAdd`,
`SecItemCopyMatching`, `SecItemUpdate`, `SecItemDelete`), with iOS's
rules for what is unique, what a query matches and which status codes come
back. expo-secure-store, Firebase and lock screens keep their secrets here.

Words used here:

- **Item**: one secret. It has a class (for example generic password or
  internet password), attributes (service, account, ...) and data.
- **Primary key**: the attributes that make an item unique. For a generic
  password: service, account and access group. For an internet password
  also server, protocol, port and path.
- **Query**: a dictionary that says which items to find and what to return
  (`kSecReturnData`, `kSecReturnAttributes`, `kSecMatchLimit`).
- **Access control**: a rule on an item, for example "Face ID is needed to
  read it".
- **Status code**: the number the call returns: 0 is success.

Measured data: hello-expo's `pommehello:///storage` and `pommehello:///lock`
screens (`crates/pomme-cli/tests/hello_expo.rs`).

## Requirements

### Requirement: Items are added, found, changed and deleted

`SecItemAdd` SHALL store an item, and SHALL return -25299 (duplicate) when
an item of the same class with the same primary key exists.
`SecItemCopyMatching`, `SecItemUpdate` and `SecItemDelete` SHALL act on the
items of the query's class whose attributes equal every attribute the query
names, and SHALL return -25300 (not found) when there are none.

#### Scenario: Replace a value

- **WHEN** expo-secure-store saves `abc123` for a key, then saves `def456` for the same key, and reads it
- **THEN** it reads `def456`

#### Scenario: Delete

- **WHEN** the app deletes the key and reads it again
- **THEN** the read finds nothing (status -25300), and the app sees null

### Requirement: Services are kept apart

Items with different services (or accounts, or access groups) SHALL be
different items, even with the same key name.

#### Scenario: Two keychain services

- **WHEN** expo-secure-store saves the same key under two `keychainService` options
- **THEN** each service reads back its own value

### Requirement: Queries say what comes back

A query SHALL return the item's data, its attributes, or both, as it asks.
With `kSecMatchLimitAll` it SHALL return an array of every match; otherwise
the first match. The caller SHALL own the returned object.

#### Scenario: All items of a service

- **WHEN** an app asks for the attributes of every generic password of one service, with `kSecMatchLimitAll`
- **THEN** it gets an array with one dictionary per item

### Requirement: Protected items need Face ID

An item made with an access control that asks for user presence,
biometry or the device passcode SHALL need a matching face to read its
data. Without enrolled Face ID, or with a face that does not match, the read
SHALL fail with -25293 (authentication failed). See `security/local-authentication`
for how the face is given.

#### Scenario: Not enrolled

- **WHEN** Face ID is not enrolled and the app reads a value saved with `requireAuthentication`
- **THEN** the read fails, and hello-expo's storage screen shows `protected error`

#### Scenario: A matching face

- **WHEN** the run uses `--biometry match` and the app reads the protected value
- **THEN** it reads `opensesame`

### Requirement: Items last for one run

Items SHALL be kept in memory for the run. Each run SHALL start with an
empty Keychain, as a fresh install does.

#### Scenario: A new run

- **WHEN** an app saves an item and is run again
- **THEN** the item is not there in the second run
