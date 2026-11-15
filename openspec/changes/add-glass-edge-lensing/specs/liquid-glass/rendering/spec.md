# Spec Delta

## ADDED Requirements

### Requirement: The backdrop bends near the rim

Inside a band near the edge of the glass, pomme SHALL show the backdrop
moved along the direction of the edge, as measured on the iOS 26 Simulator,
and SHALL draw the thin light line on the edge. The band width, the amount
of movement and the line SHALL come from the measurement, for each style
(regular, clear) and each appearance (light, dark).

#### Scenario: Stripe near the left rim

- **WHEN** regular glass is over black and orange stripes and the test samples a line across its left rim
- **THEN** each sample is within the chosen rim tolerance (at most 16 per channel) of the Simulator's

#### Scenario: Middle stays the same

- **WHEN** the test samples the middle of the glass, far from the rim
- **THEN** the samples are within 10 per channel of the Simulator's
