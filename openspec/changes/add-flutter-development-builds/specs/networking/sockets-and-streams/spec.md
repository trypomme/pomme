# Spec Delta

## ADDED Requirements

### Requirement: Listening sockets

pomme SHALL let an app's TCP socket bind to a local port, listen and
accept connections, over IPv4 and IPv6. A port the app binds SHALL be the
same port on the computer's loopback, so a program on the computer can
connect to it. A port of 0 SHALL get a free port, which `getsockname`
SHALL report. Connections the app makes to its own port SHALL arrive too.

#### Scenario: The app accepts its own connection

- **WHEN** the sockets probe binds port 0, listens, connects to that port, and accepts
- **THEN** it reports `listen=1`, and bytes sent on one end arrive at the other, as on the iOS 26 Simulator

#### Scenario: The computer connects to the app

- **WHEN** an app in pomme listens on a port and a program on the computer connects to that port on its loopback
- **THEN** the app's `accept` returns the connection, and bytes go both ways

#### Scenario: Offline

- **WHEN** the modeled network is offline
- **THEN** an app can still listen and accept connections from itself (the loopback needs no network)

### Requirement: Waiting for connections

A listening socket SHALL be readable in `poll`, `select` and kqueue when a
connection is waiting. `accept` SHALL follow `O_NONBLOCK`: it returns
`EAGAIN`, or the thread waits for a connection.

#### Scenario: Non-blocking accept

- **WHEN** the sockets probe calls `accept` on a non-blocking listening socket with no connection waiting, then connects to it and polls
- **THEN** `accept` first fails with `EAGAIN`, then `poll` reports the socket readable and `accept` returns the connection
