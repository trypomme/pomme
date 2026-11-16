// The system's proxy settings as an app reads them (tests/development_builds.rs):
// SocketRocket asks before opening a WebSocket. With no proxy configured.
#import <UIKit/UIKit.h>
#import <CFNetwork/CFNetwork.h>

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end
@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)options {
    printf("VALUE kCFProxyTypeKey %s\n", [(__bridge NSString *)kCFProxyTypeKey UTF8String]);
    printf("VALUE kCFProxyTypeNone %s\n", [(__bridge NSString *)kCFProxyTypeNone UTF8String]);
    printf("VALUE kCFProxyTypeHTTP %s\n", [(__bridge NSString *)kCFProxyTypeHTTP UTF8String]);
    printf("VALUE kCFProxyHostNameKey %s\n", [(__bridge NSString *)kCFProxyHostNameKey UTF8String]);
    printf("VALUE kCFProxyPortNumberKey %s\n", [(__bridge NSString *)kCFProxyPortNumberKey UTF8String]);
    printf("VALUE kCFNetworkProxiesHTTPEnable %s\n", [(__bridge NSString *)kCFNetworkProxiesHTTPEnable UTF8String]);
#define SHOW(k) printf("VALUE " #k " %s\n", [(__bridge NSString *)k UTF8String])
    SHOW(kCFProxyTypeHTTPS);
    SHOW(kCFProxyTypeSOCKS);
    SHOW(kCFProxyTypeFTP);
    SHOW(kCFProxyTypeAutoConfigurationURL);
    SHOW(kCFProxyTypeAutoConfigurationJavaScript);
    SHOW(kCFProxyAutoConfigurationURLKey);
    SHOW(kCFProxyAutoConfigurationJavaScriptKey);
    SHOW(kCFProxyUsernameKey);
    SHOW(kCFProxyPasswordKey);
    SHOW(kCFNetworkProxiesHTTPProxy);
    SHOW(kCFNetworkProxiesHTTPPort);
    SHOW(kCFNetworkProxiesProxyAutoConfigEnable);
    SHOW(kCFNetworkProxiesProxyAutoConfigURLString);
    NSDictionary *settings = CFBridgingRelease(CFNetworkCopySystemProxySettings());
    printf("VALUE settings.exists %d\n", settings != nil);
    printf("VALUE settings.HTTPEnable %s\n", [[settings[(__bridge NSString *)kCFNetworkProxiesHTTPEnable] description] UTF8String] ?: "(nil)");
    NSArray *proxies = CFBridgingRelease(CFNetworkCopyProxiesForURL((__bridge CFURLRef)[NSURL URLWithString:@"http://localhost:8081"], (__bridge CFDictionaryRef)settings));
    printf("VALUE proxies.count %lu\n", (unsigned long)proxies.count);
    printf("VALUE proxies.first.type %s\n", [[proxies.firstObject[(__bridge NSString *)kCFProxyTypeKey] description] UTF8String] ?: "(nil)");
    printf("VALUE proxies.first.keys %lu\n", (unsigned long)[proxies.firstObject count]);
    fflush(stdout);
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
