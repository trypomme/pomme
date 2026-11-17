# Spec Delta

## ADDED Requirements

### Requirement: Listening sockets

pomme SHALL let an app listen for TCP connections on its loopback address
(IPv4 and IPv6): `bind` (port 0 picks a free port, which `getsockname`
reports), `listen`, and `accept`. `accept` SHALL follow `O_NONBLOCK`, and
`poll`/`select` SHALL report a listening socket readable when a connection
waits.

#### Scenario: Accept a connection

- **WHEN** the sockets probe binds a socket to 127.0.0.1 port 0, listens, and connects to that port from a second socket
- **THEN** `getsockname` gives a port that is not 0, `select` reports the listening socket readable, `accept` returns a socket, and bytes sent on one end arrive on the other

#### Scenario: Nothing waiting

- **WHEN** the probe calls `accept` on a non-blocking listening socket with no connection waiting
- **THEN** `accept` fails with `EAGAIN`

#### Scenario: Not loopback

- **WHEN** the probe binds a listening socket to an address that is not loopback
- **THEN** `bind` fails as measured on the iOS 26 Simulator for an address the phone does not have (task 1.4)

### Requirement: Forwarding listening ports to the host

In a run with the real network, when the app listens on a loopback port,
pomme SHALL listen on the host's loopback address, on the same port if it
is free and on another free port if not, and pass each connection to the
app. It SHALL stop when the app closes the socket. With `--offline` or
fixtures, pomme SHALL NOT open host ports.

#### Scenario: A host program connects

- **WHEN** the sockets probe listens on a loopback port in a run with the real network, and a program on the host connects to the forwarded port and sends a line
- **THEN** the probe accepts the connection, reads the line, and its answer reaches the host program

#### Scenario: Offline

- **WHEN** the same probe runs with `--offline`
- **THEN** the probe can still connect to its own listening socket, and no port is opened on the host
