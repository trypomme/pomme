# uikit/text-input-and-keyboard Specification

## Purpose

Let people type into apps: `UITextField` for one line and `UITextView` for
many lines, with the first responder and the software keyboard working as
UIKit's do. React Native's TextInput, Flutter's TextField and SwiftUI's
TextField are built on these.

Words used here:

- **First responder**: the object that receives typed keys. At most one at a
  time.
- **Software keyboard**: the keyboard drawn on the phone's screen. Apps move
  their content out of its way.
- **Caret**: the blinking line where the next character goes.
- **UTF-16**: the text encoding iOS uses to count characters in ranges
  (`NSRange`).

Measured with the textview fixture (`crates/pomme-cli/tests/fixtures/textview/`)
and the iOS 26 Simulator.

## Requirements

### Requirement: Beginning to edit

A tap on a text field, or `becomeFirstResponder`, SHALL ask the delegate
(`textFieldShouldBeginEditing:`), make the field first responder, tell the
delegate (`textFieldDidBeginEditing:`), send `UIControlEventEditingDidBegin`,
post `UITextFieldTextDidBeginEditingNotification`, and show the keyboard.

#### Scenario: Tap a field

- **WHEN** the user taps a text field
- **THEN** it is the first responder and the keyboard comes up

### Requirement: Each keystroke asks first

Each keystroke SHALL ask `textField:shouldChangeCharactersInRange:replacementString:`
(for a text view, `textView:shouldChangeTextInRange:replacementText:`). If
the answer is yes, the text SHALL change, the field SHALL send
`EditingChanged` and the change notification SHALL be posted. Ranges SHALL
count UTF-16 units.

#### Scenario: Delegate refuses

- **WHEN** the delegate answers NO for a keystroke
- **THEN** the text does not change

### Requirement: Return and ending editing

In a text field, return SHALL ask `textFieldShouldReturn:` and then send
`EditingDidEndOnExit`. `resignFirstResponder` SHALL end editing with the
matching delegate calls and notifications, and hide the keyboard.

#### Scenario: React Native submit

- **WHEN** the user types a name in hello-expo's keyboard screen and presses return
- **THEN** the screen's submit handler runs

### Requirement: Multiline text views

A `UITextView` SHALL wrap its lines at its width (less its text container
inset, 8 pt above and below by default, and 5 pt of line padding at each
side). Return SHALL type a newline. Once the text is taller than the view,
typing SHALL scroll to keep the caret in sight. A text view that is not
editable SHALL NOT take focus. `sizeThatFits:` SHALL measure the lines and
the insets.

#### Scenario: Typing lines

- **WHEN** the user types several lines into a text view in the textview fixture
- **THEN** the text wraps, and the view scrolls so the caret stays visible

#### Scenario: Return at the end

- **WHEN** the caret is at the end of the text and the user presses return
- **THEN** the caret is on a new empty line and the content is one line taller

### Requirement: The software keyboard

While a text input is first responder, pomme SHALL show the software
keyboard with iOS 26's frame (335 pt tall on the default device, as
measured) and SHALL post the will-show, did-show, will-hide and did-hide
notifications with the keys iOS 26 sends. The keyboard SHALL slide with
UIKit's keyboard curve.

#### Scenario: Keyboard avoiding

- **WHEN** a field in hello-expo becomes first responder
- **THEN** `UIKeyboardWillShowNotification` is posted with the keyboard's end frame, and the app's KeyboardAvoidingView moves up

### Requirement: Scripted typing

`--type 'text@s'` SHALL type the text into the first responder at s seconds
of virtual time. In the text, `\n` SHALL be return and `\b` SHALL be delete.

#### Scenario: Type a name

- **WHEN** hello-expo runs with `--tap` on its field and `--type 'Ada\n@4'`
- **THEN** the field holds "Ada" and return was pressed
