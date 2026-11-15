# security/local-authentication Specification

## Purpose

Let apps ask the person for Face ID or Touch ID (`LAContext`), and answer
the way the iOS Simulator does: biometry is "not enrolled" or "enrolled",
and on an enrolled phone each prompt waits until someone presents a
matching or a non-matching face. Runs nobody watches can choose the answer
before they start.

Words used here:

- **Enrolled**: a face (or finger) is set up on the phone.
- **Prompt**: the system's "Face ID" request on screen, while the app waits
  for the answer.
- **Policy**: what the app asks for. `LAPolicy` 1 is biometrics only; 2 is
  biometrics or the device passcode.
- **LAError**: the error code of a failed request, in the
  `com.apple.LocalAuthentication` domain.

Measured data: hello-expo's `pommehello:///lock` and `pommehello:///storage`
screens (`crates/pomme-cli/tests/hello_expo.rs`).

## Requirements

### Requirement: The run chooses how biometry answers

`pomme run --biometry` SHALL set biometry: `off` (not enrolled, the
default), `enrolled` (each prompt waits for a face), `match` or `no-match`
(enrolled, and every prompt is answered at once by a matching or a
non-matching face). `--face match@T`, `--face no-match@T` and
`--face toggle-enrolled@T` SHALL present a face at virtual time T.

#### Scenario: Unattended run

- **WHEN** a run uses `--biometry match`
- **THEN** every prompt succeeds at once, with nobody present

### Requirement: The phone has no passcode

Like a fresh Simulator, the phone SHALL have no device passcode. So
`canEvaluatePolicy:error:` SHALL return YES only when biometry is enrolled,
for both policies. When it is not, the error SHALL be -7 (biometry not
enrolled) for policy 1 and -5 (passcode not set) for policy 2.

#### Scenario: Not enrolled

- **WHEN** biometry is off and the app asks `canEvaluatePolicy:` for biometrics
- **THEN** the answer is NO with LAError -7

### Requirement: The biometry type follows the device

`biometryType` SHALL be Touch ID (1) on a device with a home button and
Face ID (2) on other devices.

#### Scenario: iPhone SE

- **WHEN** the run uses `--device iphone-se` and the app reads `biometryType`
- **THEN** it is Touch ID (1)

### Requirement: Prompts wait for a face

On an enrolled phone, `evaluatePolicy:localizedReason:reply:` SHALL show a
prompt and wait. A matching face SHALL make it succeed. A non-matching face
SHALL fail it with -1 (authentication failed). Turning enrolment off while
it waits SHALL fail it as not enrolled. The reply SHALL run on a background
queue, as LocalAuthentication does.

#### Scenario: A wrong face, then the right one

- **WHEN** the lock screen asks for Face ID, a non-matching face is presented, and then the next prompt sees a matching face
- **THEN** the first shows `Face ID failed: authentication_failed` and the second `Unlocked with Face ID`

#### Scenario: Unenrolled while asking

- **WHEN** enrolment is turned off while a prompt waits
- **THEN** the lock screen shows `Face ID failed: not_enrolled`

### Requirement: Keychain reads wait too

Reading a Keychain item that needs biometry on an enrolled phone SHALL
block the reading thread while the prompt waits. The face presented SHALL
decide whether the read succeeds.

#### Scenario: expo-secure-store waits

- **WHEN** the storage screen reads its protected value on an enrolled phone and no face is presented yet
- **THEN** the line shows `protected …`, and after a matching face it shows `protected opensesame`

### Requirement: The prompt's labels are kept

`localizedReason`, `localizedFallbackTitle`, `localizedCancelTitle` and
`interactionNotAllowed` SHALL keep what the app sets and give it back.

#### Scenario: Read back a label

- **WHEN** an app sets `localizedCancelTitle` to `Not now`
- **THEN** reading it gives `Not now`
