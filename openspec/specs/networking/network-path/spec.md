# networking/network-path Specification

## Purpose

Tell apps how the phone is connected (Wi-Fi, cellular, wired or offline),
as the Network framework and SystemConfiguration report it. pomme models
one network path. The user chooses it, and can change it while the app
runs.

Words used here:

- **Path**: how the phone reaches the network right now.
- **Path monitor**: `NWPathMonitor` (`nw_path_monitor_*`). It tells the app
  the path when it starts, and every change after.
- **Reachability**: SystemConfiguration's `SCNetworkReachability`, an older
  API that answers "can I reach this host?" with flags.

## Requirements

### Requirement: The user chooses the path

`pomme run --network` SHALL accept `wifi` (the default), `cellular`,
`wired` or `offline`, and changes at virtual times
(`--network cellular --network offline@3`). The desktop window SHALL be
able to change the path too. The offline path SHALL also make requests and
connections fail.

#### Scenario: Change at 3 s

- **WHEN** the user runs `pomme run <app> --network cellular --network offline@3`
- **THEN** the app starts on cellular and becomes offline at 3 s of virtual time

### Requirement: Path monitors hear the path and its changes

A path monitor SHALL hear the current path when it starts, and every
change after, on its dispatch queue. The path SHALL report its status, its
interface type, and whether it is expensive (cellular), as the Network
framework does.
`getifaddrs` SHALL list the interface of the modeled path.

#### Scenario: Wi-Fi by default

- **WHEN** hello-expo's /network screen runs with no `--network` option
- **THEN** it shows "WIFI · connected true · reachable true · 1 updates" and "ip 192.168.1.23"

#### Scenario: Two changes

- **WHEN** the path changes to cellular at 3 s and to offline at 4 s
- **THEN** the screen shows "CELLULAR · connected true · reachable true · 2 updates" at 4 s, and "NONE · connected false · reachable false · 3 updates" at 5 s

### Requirement: Reachability follows the path

`SCNetworkReachability` targets SHALL answer for the phone's one path:
reachable on Wi-Fi or wired, reachable and WWAN on cellular, unreachable
offline. A target SHALL be a CoreFoundation object of its own type
(`CFRetain`, `CFRelease`, `CFGetTypeID` work). Scheduled on a run loop or a
queue, its callback SHALL hear every change of flags. A host-name target
SHALL also hear its first answer.

#### Scenario: Cellular flags

- **WHEN** the path is cellular and the app asks a host-name target for its flags
- **THEN** the flags say reachable and WWAN
