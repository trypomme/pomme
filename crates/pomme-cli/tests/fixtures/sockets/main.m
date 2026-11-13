// BSD sockets as a gRPC-style client uses them, for tests/sockets.rs: each
// check is printed as name=0/1 in field 1. "echo.pomme.test" port 7 is an
// echo server when the test gives the app a network.
#import <UIKit/UIKit.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <arpa/inet.h>
#include <netdb.h>
#include <poll.h>
#include <fcntl.h>
#include <unistd.h>
#include <errno.h>
#include <net/if.h>
#include <dns_sd.h>


// ---- DNS-SD and CFStream, as gRPC uses them ----

struct Lookup {
    dispatch_semaphore_t done;
    BOOL v4Loopback, v6None, v4None;
};

static void lookupReply(DNSServiceRef ref, DNSServiceFlags flags, uint32_t interface, DNSServiceErrorType err,
                        const char *host, const struct sockaddr *addr, uint32_t ttl, void *context) {
    struct Lookup *l = context;
    if (addr && addr->sa_family == AF_INET) {
        const struct sockaddr_in *a4 = (const struct sockaddr_in *)addr;
        if (err == kDNSServiceErr_NoError && (flags & kDNSServiceFlagsAdd) && a4->sin_addr.s_addr == htonl(INADDR_LOOPBACK)) l->v4Loopback = YES;
        if (err == kDNSServiceErr_NoSuchRecord) l->v4None = YES;
    }
    if (addr && addr->sa_family == AF_INET6 && err == kDNSServiceErr_NoSuchRecord) l->v6None = YES;
    if (!(flags & kDNSServiceFlagsMoreComing)) {
        DNSServiceRefDeallocate(ref);
        dispatch_semaphore_signal(l->done);
    }
}

/// Resolve with DNSServiceGetAddrInfo on a queue: (found 127.0.0.1, no IPv6, no IPv4).
static struct Lookup lookup(const char *host) {
    struct Lookup l = {dispatch_semaphore_create(0), NO, NO, NO};
    DNSServiceRef ref;
    DNSServiceErrorType e = DNSServiceGetAddrInfo(&ref, kDNSServiceFlagsTimeout | kDNSServiceFlagsReturnIntermediates, 0,
                                                  kDNSServiceProtocol_IPv4 | kDNSServiceProtocol_IPv6, host, lookupReply, &l);
    if (e == kDNSServiceErr_NoError) {
        DNSServiceSetDispatchQueue(ref, dispatch_queue_create("pomme.dnssd", DISPATCH_QUEUE_SERIAL));
        dispatch_semaphore_wait(l.done, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC));
    }
    return l;
}

struct Echo {
    CFReadStreamRef in;
    CFWriteStreamRef out;
    dispatch_semaphore_t done;
    BOOL opened, wrote;
    char got[32];
    size_t n;
};

static void readEvent(CFReadStreamRef s, CFStreamEventType type, void *context) {
    struct Echo *e = context;
    if (type == kCFStreamEventOpenCompleted) e->opened = YES;
    if (type == kCFStreamEventHasBytesAvailable) {
        CFIndex n = CFReadStreamRead(s, (UInt8 *)e->got + e->n, sizeof e->got - 1 - e->n);
        if (n > 0) e->n += n;
        if (e->n >= 9) dispatch_semaphore_signal(e->done);
    }
    if (type == kCFStreamEventErrorOccurred || type == kCFStreamEventEndEncountered) dispatch_semaphore_signal(e->done);
}

static void writeEvent(CFWriteStreamRef s, CFStreamEventType type, void *context) {
    struct Echo *e = context;
    if (type == kCFStreamEventCanAcceptBytes && !e->wrote) {
        e->wrote = CFWriteStreamWrite(s, (const UInt8 *)"cfstream!", 9) == 9;
    }
}

/// A CFStream pair to host:port with clients on a queue: what came back, or an error.
static NSString *echo(CFStringRef host, UInt32 port, CFErrorRef *error) {
    struct Echo e = {0};
    e.done = dispatch_semaphore_create(0);
    CFStreamCreatePairWithSocketToHost(NULL, host, port, &e.in, &e.out);
    CFStreamClientContext ctx = {0, &e, NULL, NULL, NULL};
    CFReadStreamSetClient(e.in, kCFStreamEventOpenCompleted | kCFStreamEventHasBytesAvailable | kCFStreamEventErrorOccurred | kCFStreamEventEndEncountered, readEvent, &ctx);
    CFWriteStreamSetClient(e.out, kCFStreamEventCanAcceptBytes | kCFStreamEventErrorOccurred, writeEvent, &ctx);
    dispatch_queue_t q = dispatch_queue_create("pomme.cfstream", DISPATCH_QUEUE_SERIAL);
    CFReadStreamSetDispatchQueue(e.in, q);
    CFWriteStreamSetDispatchQueue(e.out, q);
    NSString *got = nil;
    if (CFReadStreamOpen(e.in) && CFWriteStreamOpen(e.out)) {
        dispatch_semaphore_wait(e.done, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC));
        // The native socket is a real descriptor.
        CFDataRef native = CFReadStreamCopyProperty(e.in, kCFStreamPropertySocketNativeHandle);
        int fd = -1;
        if (native) {
            CFDataGetBytes(native, CFRangeMake(0, sizeof fd), (UInt8 *)&fd);
            CFRelease(native);
        }
        struct sockaddr_storage peer;
        socklen_t plen = sizeof peer;
        if (e.opened && fd >= 0 && getpeername(fd, (struct sockaddr *)&peer, &plen) == 0) got = [[NSString alloc] initWithBytes:e.got length:e.n encoding:NSUTF8StringEncoding];
    } else if (error) {
        *error = CFReadStreamCopyError(e.in);
    }
    CFReadStreamSetDispatchQueue(e.in, NULL);
    CFWriteStreamSetDispatchQueue(e.out, NULL);
    CFReadStreamClose(e.in);
    CFWriteStreamClose(e.out);
    CFRelease(e.in);
    CFRelease(e.out);
    return got;
}

@interface ViewController : UIViewController
@end

@implementation ViewController
- (void)viewDidLoad {
    [super viewDidLoad];
    NSMutableArray *checks = [NSMutableArray new];
    void (^check)(NSString *, BOOL) = ^(NSString *name, BOOL ok) { [checks addObject:@[name, @(ok)]]; };

    // Names and addresses.
    struct in_addr a4;
    char text[64];
    check(@"inet", inet_pton(AF_INET, "192.168.1.23", &a4) == 1 && strcmp(inet_ntop(AF_INET, &a4, text, sizeof text), "192.168.1.23") == 0 && inet_pton(AF_INET, "nope", &a4) == 0);
    struct addrinfo hints = {0}, *res = NULL;
    hints.ai_socktype = SOCK_STREAM;
    check(@"numericName", getaddrinfo("127.0.0.1", "443", &hints, &res) == 0 && res && res->ai_family == AF_INET && ntohs(((struct sockaddr_in *)res->ai_addr)->sin_port) == 443);
    if (res) freeaddrinfo(res);
    check(@"loopback", if_nametoindex("lo0") == 1);

    // A connection: resolve, connect, non-blocking send, poll, recv.
    res = NULL;
    int gai = getaddrinfo("echo.pomme.test", "7", &hints, &res);
    int fd = -1;
    BOOL online = gai == 0 && res;
    if (online) {
        fd = socket(res->ai_family, res->ai_socktype, res->ai_protocol);
        int one = 1;
        setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, sizeof one);
        online = connect(fd, res->ai_addr, res->ai_addrlen) == 0;
        freeaddrinfo(res);
    }
    if (online) {
        int err = -1;
        socklen_t len = sizeof err;
        getsockopt(fd, SOL_SOCKET, SO_ERROR, &err, &len);
        fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK);
        char buf[64] = {0};
        BOOL earlyAgain = recv(fd, buf, sizeof buf, 0) == -1 && errno == EAGAIN;
        BOOL sent = send(fd, "hello", 5, 0) == 5;
        struct pollfd p = {fd, POLLIN, 0};
        BOOL polled = poll(&p, 1, 5000) == 1 && (p.revents & POLLIN);
        ssize_t got = recv(fd, buf, sizeof buf, 0);
        check(@"echo", err == 0 && earlyAgain && sent && polled && got == 5 && memcmp(buf, "hello", 5) == 0);

        // Blocking again: write, then a recv that waits for the echo.
        fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) & ~O_NONBLOCK);
        BOOL wrote = write(fd, "pomme!", 6) == 6;
        memset(buf, 0, sizeof buf);
        ssize_t n = read(fd, buf, sizeof buf);
        check(@"blockingRead", wrote && n == 6 && memcmp(buf, "pomme!", 6) == 0);

        // select sees it writable; the peer's address is the server's.
        fd_set w;
        FD_ZERO(&w);
        FD_SET(fd, &w);
        struct timeval tv = {1, 0};
        struct sockaddr_storage peer;
        socklen_t plen = sizeof peer;
        check(@"selectAndPeer", select(fd + 1, NULL, &w, NULL, &tv) == 1 && FD_ISSET(fd, &w) && getpeername(fd, (struct sockaddr *)&peer, &plen) == 0 && peer.ss_family == AF_INET);
        close(fd);
    } else {
        // Offline: names don't resolve, connections don't open.
        int s = socket(AF_INET, SOCK_STREAM, 0);
        struct sockaddr_in to = {sizeof to, AF_INET, htons(443)};
        inet_pton(AF_INET, "142.250.70.10", &to.sin_addr);
        BOOL refused = connect(s, (struct sockaddr *)&to, sizeof to) == -1 && errno == ENETUNREACH;
        close(s);
        check(@"offline", gai == EAI_NONAME && refused);
    }

    // DNS-SD and CFStream: the echo server, or (offline) no addresses and a stream that won't open.
    struct Lookup l = lookup("echo.pomme.test");
    if (online) {
        check(@"dnssd", l.v4Loopback && l.v6None);
        check(@"cfstream", [echo(CFSTR("echo.pomme.test"), 7, NULL) isEqual:@"cfstream!"]);
    } else {
        CFErrorRef err = NULL;
        NSString *got = echo(CFSTR("echo.pomme.test"), 7, &err);
        check(@"dnssdOffline", l.v4None && l.v6None);
        check(@"cfstreamOffline", got == nil && err != NULL);
        if (err) CFRelease(err);
    }

    // A local socket pair.
    int sv[2];
    char got[8] = {0};
    BOOL pair = socketpair(AF_UNIX, SOCK_STREAM, 0, sv) == 0 && write(sv[0], "ping", 4) == 4;
    struct pollfd pp = {sv[1], POLLIN, 0};
    pair = pair && poll(&pp, 1, 0) == 1 && read(sv[1], got, sizeof got) == 4 && memcmp(got, "ping", 4) == 0;
    close(sv[0]);
    pair = pair && read(sv[1], got, sizeof got) == 0; // the peer closed: end of stream
    close(sv[1]);
    check(@"socketpair", pair);

    NSMutableArray *out = [NSMutableArray new];
    for (NSArray *c in checks) [out addObject:[NSString stringWithFormat:@"%@=%@", c[0], c[1]]];
    UITextField *f = [[UITextField alloc] initWithFrame:CGRectMake(0, 100, 300, 30)];
    f.tag = 1;
    f.text = [out componentsJoinedByString:@" "];
    [self.view addSubview:f];
}
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)options {
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    self.window.rootViewController = [ViewController new];
    [self.window makeKeyAndVisible];
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
