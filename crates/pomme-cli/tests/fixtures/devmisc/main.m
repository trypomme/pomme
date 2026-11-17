// What Debug builds' extra code asks of Foundation and UIKit
// (tests/development_builds.rs): NSCountedSet, +[NSThread callStackSymbols],
// NSCache's limits, and window levels (an alert-level window created first
// is drawn above a normal one created after it: the screen's middle is blue),
// and what React Native's error screen needs: @try/@catch, NSError's keys,
// the monospaced system font, and a date compared with nil.
#import <UIKit/UIKit.h>

@interface ProbeException : NSException
@end
@implementation ProbeException
@end

static void probeExceptions(void) {
    NSMutableString *log = [NSMutableString string];
    @try {
        @throw [NSException exceptionWithName:@"ProbeName" reason:@"probe reason" userInfo:nil];
    } @catch (NSException *e) {
        printf("VALUE caughtName %s\n", e.name.UTF8String);
        printf("VALUE caughtReason %s\n", e.reason.UTF8String);
    }
    // The first matching @catch takes it: a subclass before its superclass.
    @try {
        @throw [ProbeException exceptionWithName:@"Sub" reason:@"r" userInfo:nil];
    } @catch (ProbeException *e) {
        [log appendString:@"sub"];
    } @catch (NSException *e) {
        [log appendString:@"base"];
    } @finally {
        [log appendString:@"+finally"];
    }
    printf("VALUE catchOrder %s\n", log.UTF8String);
    // A thrown string, caught as id; a rethrow reaches the outer @catch.
    @try {
        @try {
            @throw @"a string";
        } @catch (NSException *e) {
            printf("VALUE stringCaughtAsException 1\n");
        } @catch (id other) {
            printf("VALUE stringCaughtAsId %s\n", [other isKindOfClass:NSString.class] ? [other UTF8String] : "other");
            @throw;
        }
    } @catch (id outer) {
        printf("VALUE rethrownOuter %s\n", [outer UTF8String]);
    }
    // -[NSException raise], caught.
    @try {
        [[NSException exceptionWithName:@"Raised" reason:@"by raise" userInfo:nil] raise];
    } @catch (NSException *e) {
        printf("VALUE raisedName %s\n", e.name.UTF8String);
    }
}

static void probeErrorsFontsDates(void) {
    printf("VALUE keyDescription %s\n", NSLocalizedDescriptionKey.UTF8String);
    printf("VALUE keyFailureReason %s\n", NSLocalizedFailureReasonErrorKey.UTF8String);
    printf("VALUE keyRecoverySuggestion %s\n", NSLocalizedRecoverySuggestionErrorKey.UTF8String);
    printf("VALUE keyUnderlying %s\n", NSUnderlyingErrorKey.UTF8String);
    printf("VALUE keyDebugDescription %s\n", NSDebugDescriptionErrorKey.UTF8String);
    NSError *withText = [NSError errorWithDomain:@"ProbeDomain" code:1 userInfo:@{NSLocalizedDescriptionKey: @"No script URL provided."}];
    printf("VALUE describedError %s\n", withText.localizedDescription.UTF8String);
    NSError *plain = [NSError errorWithDomain:@"ProbeDomain" code:7 userInfo:nil];
    printf("VALUE plainError %s\n", plain.localizedDescription.UTF8String);
    UIFont *mono = [UIFont monospacedSystemFontOfSize:14 weight:UIFontWeightRegular];
    printf("VALUE monoName %s\n", mono.fontName.UTF8String);
    printf("VALUE monoSize %.0f\n", mono.pointSize);
    NSDate *d = [NSDate dateWithTimeIntervalSinceReferenceDate:100];
    NSDate *none = nil;
    printf("VALUE sinceNil %.0f\n", [d timeIntervalSinceDate:none]);
}

static UIWindow *alertWindow, *normalWindow;

@interface ViewController : UIViewController
@property(nonatomic, strong) UIColor *color;
@end
@implementation ViewController
- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = self.color;
}
@end

@interface SceneDelegate : UIResponder <UIWindowSceneDelegate>
@property(nonatomic, strong) UIWindow *window;
@end
@implementation SceneDelegate
- (void)scene:(UIScene *)scene willConnectToSession:(UISceneSession *)session options:(UISceneConnectionOptions *)options {
    UIWindowScene *ws = (UIWindowScene *)scene;
    ViewController *blue = [ViewController new];
    blue.color = UIColor.blueColor;
    alertWindow = [[UIWindow alloc] initWithWindowScene:ws];
    alertWindow.windowLevel = UIWindowLevelAlert;
    alertWindow.rootViewController = blue;
    alertWindow.hidden = NO;
    ViewController *red = [ViewController new];
    red.color = UIColor.redColor;
    normalWindow = [[UIWindow alloc] initWithWindowScene:ws];
    normalWindow.rootViewController = red;
    [normalWindow makeKeyAndVisible];
    self.window = normalWindow;
    printf("VALUE alertLevel %.0f\n", alertWindow.windowLevel);
    printf("VALUE normalLevel %.0f\n", normalWindow.windowLevel);
    // What's under the screen's middle: the topmost window's view.
    CGPoint mid = CGPointMake(ws.screen.bounds.size.width / 2, ws.screen.bounds.size.height / 2);
    UIWindow *top = nil;
    for (UIWindow *w in ws.windows) {
        if (!w.hidden && (!top || w.windowLevel >= top.windowLevel)) top = w;
    }
    printf("VALUE topWindow %s\n", top == alertWindow ? "alert" : "normal");
    (void)mid;

    NSCountedSet *set = [NSCountedSet set];
    [set addObject:@"a"];
    [set addObject:@"a"];
    [set addObject:@"b"];
    printf("VALUE countedClass %s\n", [set isKindOfClass:NSCountedSet.class] ? "NSCountedSet" : "other");
    printf("VALUE countA %lu\n", (unsigned long)[set countForObject:@"a"]);
    printf("VALUE members %lu\n", (unsigned long)set.count);
    [set removeObject:@"a"];
    printf("VALUE countAAfterRemove %lu\n", (unsigned long)[set countForObject:@"a"]);
    [set removeObject:@"a"];
    printf("VALUE containsAAfterTwo %d\n", [set containsObject:@"a"]);
    printf("VALUE countMissing %lu\n", (unsigned long)[set countForObject:@"z"]);

    NSArray<NSString *> *frames = NSThread.callStackSymbols;
    printf("VALUE stackHasFrames %d\n", frames.count > 2);
    // The first frame's shape: its index, then the image this code is in.
    NSMutableArray *parts = [NSMutableArray array];
    for (NSString *p in [frames.firstObject componentsSeparatedByString:@" "]) {
        if (p.length) [parts addObject:p];
    }
    printf("VALUE firstFrameIndex %s\n", [parts[0] UTF8String]);
    printf("VALUE firstFrameImage %s\n", [parts[1] UTF8String]);
    printf("VALUE firstFrameHasPlus %d\n", [frames.firstObject containsString:@" + "]);
    printf("VALUE addressesMatch %d\n", NSThread.callStackReturnAddresses.count == frames.count);

    NSCache *cache = [NSCache new];
    cache.countLimit = 5;
    cache.totalCostLimit = 1000;
    printf("VALUE cacheLimits %lu %lu\n", (unsigned long)cache.countLimit, (unsigned long)cache.totalCostLimit);
    probeExceptions();
    probeErrorsFontsDates();
    fflush(stdout);
}
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@end
@implementation AppDelegate
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
