// Opening a link in a running app (tests/open_url.rs). While it runs, a link
// with its scheme (pommeprobe; pommeprobenoscene without scenes) is opened: on the Simulator with
// `xcrun simctl openurl` (OPEN_URL=... scripts/swiftui/metrics-simulator.sh
// openurl). It prints which callback got the link, the URL, every option
// that came with it, the app's state then, and the lifecycle notifications
// around it. openurl has a scene delegate; openurlnoscene (the same source,
// built with NO_SCENES) has none. PROBE_VARIANT=self: the app opens its own
// link at 2 s (UIApplication openURL:options:completionHandler:) instead.
#import <UIKit/UIKit.h>

static NSMutableArray<NSString *> *events;
static NSUInteger eventsAtLink = 0;

static const char *str(id v) {
    if (v == nil) return "(nil)";
    if ([v isKindOfClass:NSString.class]) return [v UTF8String];
    return [NSString stringWithFormat:@"%@:%@", NSStringFromClass([v class]), v].UTF8String;
}

static void listen(void) {
    NSDictionary *names = @{
        UIApplicationWillResignActiveNotification: @"willResignActive",
        UIApplicationDidBecomeActiveNotification: @"didBecomeActive",
        UIApplicationWillEnterForegroundNotification: @"willEnterForeground",
        UIApplicationDidEnterBackgroundNotification: @"didEnterBackground",
    };
    for (NSString *n in names) {
        [NSNotificationCenter.defaultCenter addObserverForName:n object:nil queue:nil usingBlock:^(NSNotification *note) {
            [events addObject:names[n]];
        }];
    }
}

// What happened around the link: the notifications before it (since launch) and in the 2 s after.
static void linkArrived(const char *callback, NSURL *url) {
    printf("VALUE callback %s\n", callback);
    printf("VALUE url %s\n", url.absoluteString.UTF8String);
    printf("VALUE appState %ld\n", (long)UIApplication.sharedApplication.applicationState);
    printf("VALUE eventsBeforeLink %s\n", [events componentsJoinedByString:@","].UTF8String);
    eventsAtLink = events.count;
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_SEC), dispatch_get_main_queue(), ^{
        NSArray *after = [events subarrayWithRange:NSMakeRange(eventsAtLink, events.count - eventsAtLink)];
        printf("VALUE eventsAfterLink %s\n", after.count ? [after componentsJoinedByString:@","].UTF8String : "(none)");
        fflush(stdout);
    });
    fflush(stdout);
}

@interface ViewController : UIViewController
@end
@implementation ViewController
- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;
}
@end

#ifndef NO_SCENES
@interface SceneDelegate : UIResponder <UIWindowSceneDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation SceneDelegate
- (void)scene:(UIScene *)scene willConnectToSession:(UISceneSession *)session options:(UISceneConnectionOptions *)options {
    printf("VALUE launchContexts %lu\n", (unsigned long)options.URLContexts.count);
    self.window = [[UIWindow alloc] initWithWindowScene:(UIWindowScene *)scene];
    self.window.rootViewController = [ViewController new];
    [self.window makeKeyAndVisible];
}
- (void)scene:(UIScene *)scene openURLContexts:(NSSet<UIOpenURLContext *> *)contexts {
    printf("VALUE contexts %lu\n", (unsigned long)contexts.count);
    UIOpenURLContext *c = contexts.anyObject;
    UISceneOpenURLOptions *o = c.options;
    printf("VALUE contextClass %s\n", NSStringFromClass(c.class).UTF8String);
    printf("VALUE optionsClass %s\n", NSStringFromClass(o.class).UTF8String);
    printf("VALUE sourceApplication %s\n", str(o.sourceApplication));
    printf("VALUE annotation %s\n", str(o.annotation));
    printf("VALUE openInPlace %d\n", o.openInPlace);
    printf("VALUE eventAttribution %s\n", str(o.eventAttribution));
    printf("VALUE sceneState %ld\n", (long)scene.activationState);
    linkArrived("scene:openURLContexts:", c.URL);
}
@end
#endif

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)options {
    printf("VALUE launchOptionsURL %s\n", str(options[UIApplicationLaunchOptionsURLKey]));
#ifdef NO_SCENES
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    self.window.rootViewController = [ViewController new];
    [self.window makeKeyAndVisible];
#endif
    if ([NSProcessInfo.processInfo.environment[@"PROBE_VARIANT"] isEqualToString:@"self"]) {
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_SEC), dispatch_get_main_queue(), ^{
#ifdef NO_SCENES
            NSURL *url = [NSURL URLWithString:@"pommeprobenoscene://open/path?x=1"];
#else
            NSURL *url = [NSURL URLWithString:@"pommeprobe://open/path?x=1"];
#endif
            [app openURL:url options:@{} completionHandler:^(BOOL ok) {
                printf("VALUE selfOpenSucceeded %d\n", ok);
                fflush(stdout);
            }];
        });
    }
    fflush(stdout);
    return YES;
}
- (BOOL)application:(UIApplication *)app openURL:(NSURL *)url options:(NSDictionary<UIApplicationOpenURLOptionsKey, id> *)options {
    NSArray *keys = [options.allKeys sortedArrayUsingSelector:@selector(compare:)];
    printf("VALUE optionKeys %s\n", keys.count ? [keys componentsJoinedByString:@","].UTF8String : "(none)");
    for (NSString *k in keys) printf("VALUE option.%s %s\n", k.UTF8String, str(options[k]));
    linkArrived("application:openURL:options:", url);
    return YES;
}
@end

int main(int argc, char *argv[]) {
    events = [NSMutableArray new];
    listen();
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
