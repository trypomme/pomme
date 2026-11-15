# Spec Delta

## MODIFIED Requirements

### Requirement: Glass in a container draws itself

A `UIGlassContainerEffect` view SHALL paint the glass of all the glass
effect views inside its content view together, as one group. Each child
SHALL keep its own frame, corners, tint and subviews. A child SHALL NOT
paint its glass a second time.

#### Scenario: Two circles in a container

- **WHEN** the probe puts two 80 pt capsule glass views in a container's content view
- **THEN** both circles are at the frames in `fixtures/glass/ios26.txt` (within 0.5 pt), and the container paints their glass once
