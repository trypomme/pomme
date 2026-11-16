# Proposal: Bar button items on glass

## Why

Most apps put buttons in the navigation bar (`navigationItem.rightBarButtonItem`,
`leftBarButtonItems`) and in a toolbar at the bottom (`toolbarItems`). In
iOS 26 these buttons sit on glass capsules. Items next to each other share
one capsule. A `.prominent` item is tinted glass.

pomme does not show bar button items at all yet: the class exists, but the
navigation bar draws only the title and the back button, and there is no
toolbar. So most navigation-based apps are missing their buttons.

## What Changes

- The navigation bar shows its leading and trailing bar button items:
  system items (add, done, edit, ...), title items, image items, and custom
  views. Taps call their action.
- The navigation controller shows a toolbar when `isToolbarHidden` is false
  and the top screen has `toolbarItems`.
- Under Liquid Glass, items sit on glass capsules: on iOS 26.0 each item on
  its own (recorded: two items are two capsules, 12 pt apart);
  `hidesSharedBackground` leaves an item none. `.prominent` items, and the
  system Done item, are blue glass.
- Before iOS 26, items are drawn as tinted text or images with no
  background, as on iOS 18.
- VoiceOver sees each item as a button.

Out of scope:

- Menus on bar button items.
- `UIBarButtonItemGroup` overflow ("..." when items do not fit).
- Search bars in the navigation bar.

## Capabilities

### New Capabilities

- `uikit/bar-button-items`: bar button items in the navigation bar and the
  toolbar, in both designs.

### Modified Capabilities

(none)

## Impact

- `crates/pomme-hle/src/navigation.rs`: items in `BarLook`, the toolbar,
  hit testing for items.
- `crates/pomme-hle/src/render.rs`: draw items and their glass capsules.
- `crates/pomme-hle/src/accessibility.rs`: items as buttons.
- Depends on `render-system-chrome-as-glass` for the glass recipe of bar
  capsules.
- New probe `crates/pomme-cli/tests/fixtures/baritems/` and test
  `crates/pomme-cli/tests/bar_items.rs`.
