# networking/sockets-and-streams Specification

## Purpose

Let apps that talk to servers themselves (gRPC, WebSockets, database
clients such as Firestore) open TCP connections. pomme provides BSD
sockets, CFNetwork's socket streams and DNS Service Discovery lookups, all
over the same host interface as URL loading.

Words used here:

- **Socket**: a connection end that a program reads from and writes to.
- **Non-blocking**: a socket that answers "try again later" (`EAGAIN`)
  instead of waiting.
- **Socket stream**: CFNetwork's pair of a read stream and a write stream
  over one TCP connection (`CFStreamCreatePairWithSocketToHost`).
- **DNS-SD**: DNS Service Discovery (`<dns_sd.h>`). gRPC resolves host
  names with `DNSServiceGetAddrInfo`.

Probe: `crates/pomme-cli/tests/fixtures/sockets/main.m`, run online against
an echo server on the host's loopback, and offline.

## Requirements

### Requirement: TCP sockets over the host

pomme SHALL provide TCP sockets over IPv4 and IPv6 and local socket pairs:
resolve names, connect, send and receive (also their vector forms), socket
options and names. A TCP connection SHALL be the host's. Connecting SHALL
complete at once. Reads and writes SHALL follow `O_NONBLOCK`: they return
`EAGAIN`, or the thread waits.

#### Scenario: Echo server

- **WHEN** the sockets probe runs with a network that sends every connection to an echo server
- **THEN** it reports `inet=1 numericName=1 loopback=1 echo=1 blockingRead=1 selectAndPeer=1 dnssd=1 cfstream=1 socketpair=1`

### Requirement: Offline sockets fail as on a phone

With no network, or with the modeled network offline, names SHALL NOT
resolve and connections SHALL NOT open. Socket pairs SHALL still work.

#### Scenario: Probe offline

- **WHEN** the sockets probe runs with no network
- **THEN** it reports `inet=1 numericName=1 loopback=1 offline=1 dnssdOffline=1 cfstreamOffline=1 socketpair=1`

### Requirement: poll and select

`poll` and `select` SHALL cover sockets, pipes and files together. When
every thread waits on the network, pomme SHALL wait for it in real time
and move virtual time on by the same amount.

#### Scenario: Wait for an answer

- **WHEN** the probe calls `select` on a socket that the echo server answers
- **THEN** `select` returns with the socket readable, and the reply can be read

### Requirement: DNS-SD lookups

`DNSServiceGetAddrInfo` SHALL deliver each address on the request's
dispatch queue with `kDNSServiceFlagsAdd`, every one but the last also
with `kDNSServiceFlagsMoreComing`. With `kDNSServiceFlagsReturnIntermediates`,
a family with no address SHALL be reported as `kDNSServiceErr_NoSuchRecord`.
With `kDNSServiceFlagsTimeout`, a failed lookup SHALL report
`kDNSServiceErr_Timeout`; without it, the request SHALL wait.

#### Scenario: Lookup offline with a timeout

- **WHEN** the probe looks up a name offline with `kDNSServiceFlagsTimeout`
- **THEN** its callback gets `kDNSServiceErr_Timeout`

### Requirement: CFNetwork socket streams

pomme SHALL provide the socket streams that CoreFoundation asks CFNetwork
for. Opening either stream SHALL connect. The streams SHALL signal
`kCFStreamEventHasBytesAvailable` and `kCFStreamEventCanAcceptBytes`, each
once until the stream is read or written again. Reads and writes SHALL
block until they can go on. The connection SHALL close when both streams
have closed.

#### Scenario: Stream round trip

- **WHEN** the probe opens a stream pair to the echo server, writes a message and waits for bytes
- **THEN** the read stream gets the same message back

### Requirement: Firestore's transport works

Together, sockets, DNS-SD, streams and TLS over them SHALL be enough for
gRPC as the Firebase Apple SDK uses it.

#### Scenario: Firestore over gRPC

- **WHEN** hello-expo's /firebase screen runs with a live network and a valid `GoogleService-Info.plist`
- **THEN** the Firestore line shows "cache hello · server same"
