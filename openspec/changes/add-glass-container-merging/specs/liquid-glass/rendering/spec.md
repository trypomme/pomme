# Spec Delta

## ADDED Requirements

### Requirement: Close glass in a container joins into one shape

When two glass pieces in the same container are closer than the container's
`spacing`, pomme SHALL paint them as one shape, with a smooth bridge between
them, as measured on the iOS 26 Simulator. When they are farther apart than
`spacing`, pomme SHALL paint them as separate shapes. The backdrop SHALL be
blurred once for the whole group.

#### Scenario: Bridge between two circles

- **WHEN** two 80 pt circles are 20 pt apart in a container with `spacing` 40
- **THEN** the pixels between them are glass, and each sample across the bridge is within 10 per channel of the Simulator's

#### Scenario: Pieces too far apart

- **WHEN** two 80 pt circles are 60 pt apart in a container with `spacing` 40
- **THEN** the pixels in the middle of the gap show the backdrop, not glass
