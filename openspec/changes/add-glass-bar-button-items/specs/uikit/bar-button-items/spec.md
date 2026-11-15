# Spec Delta

## Purpose

Show bar button items in the navigation bar and in the navigation
controller's toolbar, in both the iOS 26 design (items on glass capsules)
and the iOS 18 design, as measured on the Simulator.

## ADDED Requirements

### Requirement: The navigation bar shows its items

pomme SHALL show the top screen's leading and trailing bar button items in
the navigation bar, at the frames measured on the iOS 26 Simulator for each
design. Items SHALL show their system image, title, image or custom view. A
tap on an item SHALL send its action to its target.

#### Scenario: One trailing add item

- **WHEN** the top screen sets `navigationItem.rightBarButtonItem` to a system "add" item
- **THEN** the item is drawn at the measured frame, and tapping it runs its action

### Requirement: Items sit on glass under Liquid Glass

Under Liquid Glass, items SHALL sit on glass capsules. Items next to each
other SHALL share one capsule. An item with `hidesSharedBackground` set, or
a fixed space between items, SHALL start a new capsule. A `.prominent` item
SHALL be on tinted glass.

#### Scenario: Two trailing items

- **WHEN** the top screen has two trailing items under Liquid Glass
- **THEN** one capsule surrounds both, as on the Simulator

#### Scenario: Prominent item

- **WHEN** an item's style is `.prominent`
- **THEN** its capsule is tinted with the tint color, and its pixels are within 10 per channel of the Simulator's

### Requirement: Items before iOS 26 have no background

In the iOS 18 design, items SHALL be drawn as tinted titles or images with
no capsule, at the frames measured with `UIDesignRequiresCompatibility`.

#### Scenario: Compatibility design

- **WHEN** the app opts out of the new design and has one trailing item
- **THEN** the item is tinted text with no background

### Requirement: The toolbar shows the top screen's items

When the navigation controller's `isToolbarHidden` is false and the top
screen has `toolbarItems`, pomme SHALL show a toolbar at the bottom with
those items, lay out flexible and fixed spaces, and add the toolbar's height
to the content's bottom safe area. Under Liquid Glass the toolbar's items
SHALL sit on glass capsules.

#### Scenario: Toolbar with three items

- **WHEN** the top screen has three toolbar items separated by flexible spaces
- **THEN** the items are at the measured frames, on glass capsules

### Requirement: VoiceOver sees items as buttons

Each bar button item SHALL be a button in the accessibility tree, with its
title, its accessibility label, or iOS's name for its system item.

#### Scenario: Add item

- **WHEN** a screen reader reads the navigation bar with a system "add" item
- **THEN** it reads a button named "Add"
