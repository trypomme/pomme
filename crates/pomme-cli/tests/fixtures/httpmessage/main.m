// CFHTTPMessage (tests/development_builds.rs): building and serializing a
// request, and parsing a response fed in pieces, as SocketRocket does for a
// WebSocket handshake. Bytes are printed escaped (\r \n).
#import <UIKit/UIKit.h>
#import <CFNetwork/CFNetwork.h>

static NSString *escaped(NSData *d) {
    NSString *s = [[NSString alloc] initWithData:d encoding:NSUTF8StringEncoding];
    return [[s stringByReplacingOccurrencesOfString:@"\r" withString:@"\\r"] stringByReplacingOccurrencesOfString:@"\n" withString:@"\\n"];
}

static void report(void) {
    NSURL *url = [NSURL URLWithString:@"http://localhost:8081/message?device=x&app=y"];
    CFHTTPMessageRef req = CFHTTPMessageCreateRequest(NULL, CFSTR("GET"), (__bridge CFURLRef)url, kCFHTTPVersion1_1);
    CFHTTPMessageSetHeaderFieldValue(req, CFSTR("Host"), CFSTR("localhost:8081"));
    CFHTTPMessageSetHeaderFieldValue(req, CFSTR("Upgrade"), CFSTR("websocket"));
    CFHTTPMessageSetHeaderFieldValue(req, CFSTR("Connection"), CFSTR("Upgrade"));
    CFHTTPMessageSetHeaderFieldValue(req, CFSTR("Sec-WebSocket-Key"), CFSTR("dGhlIHNhbXBsZSBub25jZQ=="));
    CFHTTPMessageSetHeaderFieldValue(req, CFSTR("Sec-WebSocket-Version"), CFSTR("13"));
    CFHTTPMessageSetHeaderFieldValue(req, CFSTR("connection"), CFSTR("Upgrade, keep-alive"));
    printf("VALUE request %s\n", escaped(CFBridgingRelease(CFHTTPMessageCopySerializedMessage(req))).UTF8String);
    printf("VALUE request.isRequest %d\n", CFHTTPMessageIsRequest(req));
    printf("VALUE request.method %s\n", [CFBridgingRelease(CFHTTPMessageCopyRequestMethod(req)) UTF8String]);
    printf("VALUE request.url %s\n", [[CFBridgingRelease(CFHTTPMessageCopyRequestURL(req)) absoluteString] UTF8String]);
    CFRelease(req);

    NSURL *bare = [NSURL URLWithString:@"http://localhost:8081"];
    CFHTTPMessageRef req2 = CFHTTPMessageCreateRequest(NULL, CFSTR("GET"), (__bridge CFURLRef)bare, kCFHTTPVersion1_1);
    printf("VALUE request.bare %s\n", escaped(CFBridgingRelease(CFHTTPMessageCopySerializedMessage(req2))).UTF8String);
    CFRelease(req2);

    CFHTTPMessageRef resp = CFHTTPMessageCreateEmpty(NULL, false);
    const char *part1 = "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConn";
    const char *part2 = "ection: Upgrade\r\nSec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\nEXTRA";
    printf("VALUE append1 %d\n", CFHTTPMessageAppendBytes(resp, (const UInt8 *)part1, strlen(part1)));
    printf("VALUE complete1 %d\n", CFHTTPMessageIsHeaderComplete(resp));
    printf("VALUE append2 %d\n", CFHTTPMessageAppendBytes(resp, (const UInt8 *)part2, strlen(part2)));
    printf("VALUE complete2 %d\n", CFHTTPMessageIsHeaderComplete(resp));
    printf("VALUE status %ld\n", (long)CFHTTPMessageGetResponseStatusCode(resp));
    printf("VALUE statusLine %s\n", [CFBridgingRelease(CFHTTPMessageCopyResponseStatusLine(resp)) UTF8String]);
    printf("VALUE header.upgrade %s\n", [CFBridgingRelease(CFHTTPMessageCopyHeaderFieldValue(resp, CFSTR("upgrade"))) UTF8String] ?: "(nil)");
    printf("VALUE header.accept %s\n", [CFBridgingRelease(CFHTTPMessageCopyHeaderFieldValue(resp, CFSTR("SEC-WEBSOCKET-ACCEPT"))) UTF8String] ?: "(nil)");
    printf("VALUE header.missing %s\n", [CFBridgingRelease(CFHTTPMessageCopyHeaderFieldValue(resp, CFSTR("X-None"))) UTF8String] ?: "(nil)");
    printf("VALUE headers.count %lu\n", (unsigned long)[CFBridgingRelease(CFHTTPMessageCopyAllHeaderFields(resp)) count]);
    printf("VALUE body %s\n", escaped(CFBridgingRelease(CFHTTPMessageCopyBody(resp))).UTF8String);
    printf("VALUE response.isRequest %d\n", CFHTTPMessageIsRequest(resp));
    CFRelease(resp);
    fflush(stdout);
}

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end
@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)options {
    report();
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    self.window.rootViewController = [UIViewController new];
    [self.window makeKeyAndVisible];
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
