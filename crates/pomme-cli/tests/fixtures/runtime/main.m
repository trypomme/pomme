// The C runtime under an app, for tests/runtime.rs: atomics across threads,
// scanning and formatting, calendar time, C strings, and how the app was
// launched (arguments, environment, NSArgumentDomain). Each check is printed
// as name=0/1 in field 1.
#import <UIKit/UIKit.h>
#include <pthread.h>
#include <sched.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <time.h>

// The exclusive monitor across a thread switch: a load-exclusive, then a
// yield to a thread that writes the same word. The kernel clears the monitor
// on a context switch, so the store-exclusive after it fails (and lock-free
// code retries) instead of overwriting the other thread's write.
static unsigned long word;

static void *writer(void *arg) {
    word = 42;
    return NULL;
}
@interface ViewController : UIViewController
@end

@implementation ViewController
- (void)viewDidLoad {
    [super viewDidLoad];
    NSMutableArray *checks = [NSMutableArray new];
    void (^check)(NSString *, BOOL) = ^(NSString *name, BOOL ok) { [checks addObject:@[name, @(ok)]]; };

    // Atomics: a store-exclusive fails after a switch to a thread that wrote the word.
    pthread_t t;
    pthread_create(&t, NULL, writer, NULL);
    unsigned long seen = __builtin_arm_ldrex(&word);
    sched_yield();
    int failed = __builtin_arm_strex(seen + 1, &word);
    pthread_join(t, NULL);
    check(@"atomics", seen == 0 && failed == 1 && word == 42);

    // sscanf, as gRPC and HTTP parsers use it.
    int major = 0, minor = 0, status = 0, n = 0;
    char key[16] = {0}, value[16] = {0};
    double f = 0;
    BOOL scanned = sscanf("HTTP/1.1 200", "HTTP/%d.%d %d", &major, &minor, &status) == 3 && major == 1 && minor == 1 && status == 200;
    scanned = scanned && sscanf("name=pomme; 2.5", "%15[^=]=%15[^;]; %lf%n", key, value, &f, &n) == 3 && strcmp(key, "name") == 0 && strcmp(value, "pomme") == 0 && f == 2.5 && n == 15;
    scanned = scanned && sscanf("", "%d", &major) == EOF && sscanf("x", "%d", &major) == 0;
    check(@"sscanf", scanned);

    // Calendar time: the clock the test set, broken down and formatted.
    time_t now = time(NULL);
    struct tm tm;
    gmtime_r(&now, &tm);
    char when[64];
    strftime(when, sizeof when, "%Y-%m-%d %a %j", &tm);
    struct tm back = tm;
    check(@"time", now >= 1794553712 && now < 1794553712 + 60 && strcmp(when, "2026-11-13 Fri 317") == 0 && timegm(&back) == now);
    check(@"strftimeFits", strftime(when, 5, "%Y-%m", &tm) == 0);

    // C strings: long, and not UTF-8.
    size_t big = 200000;
    char *s = malloc(big + 1);
    memset(s, 'a', big);
    s[big] = 0;
    BOOL strings = strlen(s) == big;
    free(s);
    strings = strings && strlen("caf\xe9\xff") == 5 && strcmp("\xff", "a") > 0 && strcmp("a", "\xff") < 0;
    check(@"strings", strings);

    // String encodings: UTF-32 and UTF-16 lengths, and bytes in UTF-32.
    NSString *name = @"_ev";
    UTF32Char first = 0;
    NSUInteger used = 0;
    BOOL got = [name getBytes:&first maxLength:4 usedLength:&used encoding:NSUTF32LittleEndianStringEncoding options:0 range:NSMakeRange(0, 1) remainingRange:NULL];
    check(@"encodings", [name lengthOfBytesUsingEncoding:NSUTF32StringEncoding] == 12 && [@"café" lengthOfBytesUsingEncoding:NSUTF16StringEncoding] == 8 &&
                            [@"café" lengthOfBytesUsingEncoding:NSUTF8StringEncoding] == 5 && [@"café" lengthOfBytesUsingEncoding:NSASCIIStringEncoding] == 0 &&
                            got && used == 4 && first == '_' && [name dataUsingEncoding:NSUTF32StringEncoding].length == 16);

    // CFPreferences and NSUserDefaults are one store.
    CFPreferencesSetAppValue(CFSTR("pommeCF"), (__bridge CFNumberRef)@7, kCFPreferencesCurrentApplication);
    [NSUserDefaults.standardUserDefaults setObject:@"ns" forKey:@"pommeNS"];
    CFStringRef fromNS = CFPreferencesCopyAppValue(CFSTR("pommeNS"), kCFPreferencesCurrentApplication);
    Boolean valid = false;
    check(@"preferences", CFPreferencesAppSynchronize(kCFPreferencesCurrentApplication) && [NSUserDefaults.standardUserDefaults integerForKey:@"pommeCF"] == 7 &&
                              fromNS && CFEqual(fromNS, CFSTR("ns")) && CFPreferencesGetAppIntegerValue(CFSTR("pommeCF"), kCFPreferencesCurrentApplication, &valid) == 7 && valid);
    if (fromNS) CFRelease(fromNS);

    // How the app was launched.
    NSArray *args = NSProcessInfo.processInfo.arguments;
    check(@"arguments", args.count == 4 && [args[1] isEqual:@"-PommeFlag"] && [args[2] isEqual:@"-PommeLevel"] && [args[3] isEqual:@"3"]);
    check(@"argumentDomain", [NSUserDefaults.standardUserDefaults integerForKey:@"PommeLevel"] == 3);
    check(@"environment", [NSProcessInfo.processInfo.environment[@"POMME_GREETING"] isEqual:@"hello"] && strcmp(getenv("POMME_GREETING"), "hello") == 0);

    NSMutableArray *out = [NSMutableArray new];
    for (NSArray *c in checks) [out addObject:[NSString stringWithFormat:@"%@=%@", c[0], c[1]]];
    UITextField *field = [[UITextField alloc] initWithFrame:CGRectMake(0, 100, 300, 30)];
    field.tag = 1;
    field.text = [out componentsJoinedByString:@" "];
    [self.view addSubview:field];

    // Dispatch sources, which need the run loop going: a repeating timer that
    // cancels itself after three fires, then a data source whose merges
    // before it's resumed arrive together. The line gains sources=0/1.
    dispatch_queue_t q = dispatch_queue_create("pomme.sources", DISPATCH_QUEUE_SERIAL);
    __block unsigned long fires = 0, calls = 0, total = 0;
    dispatch_source_t timer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, q);
    dispatch_source_set_timer(timer, dispatch_time(DISPATCH_TIME_NOW, 100 * NSEC_PER_MSEC), 100 * NSEC_PER_MSEC, 0);
    dispatch_source_set_event_handler(timer, ^{
        fires += dispatch_source_get_data(timer);
        if (fires >= 3) dispatch_source_cancel(timer);
    });
    dispatch_source_set_cancel_handler(timer, ^{
        dispatch_source_t add = dispatch_source_create(DISPATCH_SOURCE_TYPE_DATA_ADD, 0, 0, q);
        dispatch_source_set_event_handler(add, ^{
            calls++;
            total += dispatch_source_get_data(add);
        });
        dispatch_source_merge_data(add, 2);
        dispatch_source_merge_data(add, 3);
        dispatch_resume(add);
        dispatch_async(q, ^{
            BOOL ok = fires == 3 && dispatch_source_testcancel(timer) && calls == 1 && total == 5;
            dispatch_async(dispatch_get_main_queue(), ^{
                field.text = [field.text stringByAppendingFormat:@" sources=%d", ok];
            });
        });
    });
    dispatch_resume(timer);
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
