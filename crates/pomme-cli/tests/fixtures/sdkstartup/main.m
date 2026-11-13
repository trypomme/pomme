// What SDKs such as Firebase do at launch, for tests/sdk_startup.rs: each
// check is printed as name=0/1 in field 1.
#import <UIKit/UIKit.h>
#include <sys/sysctl.h>

@protocol PommeProbe <NSObject>
@end

@interface ViewController : UIViewController
@end

@implementation ViewController
- (void)viewDidLoad {
    [super viewDidLoad];
    NSMutableArray *checks = [NSMutableArray new];
    void (^check)(NSString *, BOOL) = ^(NSString *name, BOOL ok) { [checks addObject:@[name, @(ok)]]; };

    // Property lists: written and read back, mutable when asked for.
    NSString *dir = NSTemporaryDirectory();
    NSString *path = [dir stringByAppendingPathComponent:@"config.plist"];
    NSDictionary *config = @{@"API_KEY": @"abc", @"IS_ENABLED": @YES, @"PORT": @8080, @"LIST": @[@"a", @"b"]};
    check(@"plistWrite", [config writeToFile:path atomically:YES]);
    NSDictionary *read = [NSDictionary dictionaryWithContentsOfFile:path];
    check(@"plistRead", [read isEqual:config]);
    NSMutableDictionary *mutable = [NSMutableDictionary dictionaryWithContentsOfFile:path];
    mutable[@"EXTRA"] = @1;
    check(@"plistMutable", mutable.count == 5);
    check(@"plistNotArray", [NSArray arrayWithContentsOfFile:path] == nil);
    check(@"plistMissing", [NSDictionary dictionaryWithContentsOfFile:[dir stringByAppendingPathComponent:@"none.plist"]] == nil);
    NSError *error = nil;
    NSData *binary = [NSPropertyListSerialization dataWithPropertyList:config format:NSPropertyListBinaryFormat_v1_0 options:0 error:&error];
    NSPropertyListFormat format = 0;
    id back = [NSPropertyListSerialization propertyListWithData:binary options:NSPropertyListImmutable format:&format error:&error];
    check(@"plistBinary", [back isEqual:config] && format == NSPropertyListBinaryFormat_v1_0);
    id bad = [NSPropertyListSerialization propertyListWithData:[@"not a plist <" dataUsingEncoding:NSUTF8StringEncoding] options:0 format:NULL error:&error];
    check(@"plistBadData", bad == nil && error.code == 3840);

    // NSScanner, as Firebase checks its app ID ("1:1234:ios:abcd").
    NSScanner *scanner = [NSScanner scannerWithString:@"1:1234:ios:ab12"];
    scanner.charactersToBeSkipped = nil;
    NSString *version = nil, *platform = nil;
    long long number = 0;
    BOOL scanned = [scanner scanCharactersFromSet:NSCharacterSet.decimalDigitCharacterSet intoString:&version] && [scanner scanString:@":" intoString:NULL] && [scanner scanLongLong:&number] && [scanner scanString:@":" intoString:NULL] && [scanner scanUpToString:@":" intoString:&platform] && [scanner scanString:@":" intoString:NULL];
    unsigned hex = 0;
    check(@"scanner", scanned && [version isEqual:@"1"] && number == 1234 && [platform isEqual:@"ios"] && [scanner scanHexInt:&hex] && hex == 0xab12 && scanner.isAtEnd);
    NSScanner *numbers = [NSScanner scannerWithString:@"  -2.5e1 x"];
    double d = 0;
    check(@"scanDouble", [numbers scanDouble:&d] && d == -25.0 && ![numbers scanInt:NULL] && numbers.scanLocation == 8);

    // NSCalendar: the device's (Gregorian, GMT); components, arithmetic.
    NSCalendar *cal = NSCalendar.currentCalendar;
    NSDateComponents *c = [NSDateComponents new];
    c.year = 2026; c.month = 1; c.day = 31; c.hour = 9;
    NSDate *jan31 = [cal dateFromComponents:c];
    NSDate *feb = [cal dateByAddingUnit:NSCalendarUnitMonth value:1 toDate:jan31 options:0];
    NSDateComponents *f = [cal components:NSCalendarUnitYear | NSCalendarUnitMonth | NSCalendarUnitDay | NSCalendarUnitHour | NSCalendarUnitWeekday fromDate:feb];
    check(@"calendar", [cal.calendarIdentifier isEqual:NSCalendarIdentifierGregorian] && f.month == 2 && f.day == 28 && f.hour == 9 && f.weekday == 7);
    check(@"calendarDays", [cal rangeOfUnit:NSCalendarUnitDay inUnit:NSCalendarUnitMonth forDate:feb].length == 28);
    NSDateComponents *gap = [cal components:NSCalendarUnitDay fromDate:jan31 toDate:feb options:0];
    check(@"calendarDifference", gap.day == 28);
    check(@"timeZone", [NSTimeZone.localTimeZone.name isEqual:@"GMT"] && [NSTimeZone timeZoneWithName:@"Australia/Sydney"].secondsFromGMT >= 36000 && [NSTimeZone timeZoneForSecondsFromGMT:19800].name.length > 0 && [NSTimeZone timeZoneWithName:@"Nowhere/Special"] == nil);

    // sysctl: the model, CPUs, and no debugger.
    char machine[64] = {0};
    size_t size = 0;
    sysctlbyname("hw.machine", NULL, &size, NULL, 0);
    BOOL machineOK = size > 0 && size <= sizeof(machine) && sysctlbyname("hw.machine", machine, &size, NULL, 0) == 0;
    check(@"sysctlMachine", machineOK && strncmp(machine, "iPhone", 6) == 0);
    int cpus = 0;
    size = sizeof(cpus);
    check(@"sysctlCPUs", sysctlbyname("hw.ncpu", &cpus, &size, NULL, 0) == 0 && cpus > 0);
    struct kinfo_proc info = {0};
    size = sizeof(info);
    int mib[4] = {CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()};
    check(@"sysctlNoDebugger", sysctl(mib, 4, &info, &size, NULL, 0) == 0 && (info.kp_proc.p_flag & P_TRACED) == 0);
    check(@"sysctlUnknown", sysctlbyname("no.such.name", NULL, &size, NULL, 0) == -1);

    // Protocols by name; NSURL's parts; UIKit's constants.
    check(@"protocol", [NSStringFromProtocol(@protocol(PommeProbe)) isEqual:@"PommeProbe"] && NSProtocolFromString(@"PommeProbe") == @protocol(PommeProbe));
    NSURL *url = [NSURL URLWithString:@"https://user@example.com:8443/a/b.txt?q=1#top"];
    check(@"url", url.port.intValue == 8443 && [url.query isEqual:@"q=1"] && [url.fragment isEqual:@"top"] && [url.user isEqual:@"user"] && [url.pathComponents isEqual:@[@"/", @"a", @"b.txt"]]);
    NSURL *file = [[NSURL fileURLWithPath:dir] URLByAppendingPathComponent:@"logs" isDirectory:YES];
    check(@"urlDirectory", [file.absoluteString hasSuffix:@"/logs/"] && [[[file URLByAppendingPathComponent:@"x"] URLByAppendingPathExtension:@"json"].lastPathComponent isEqual:@"x.json"]);
    check(@"backgroundTaskInvalid", UIBackgroundTaskInvalid == 0);

    // An operation queue running its operations on a dispatch queue of its own.
    NSOperationQueue *queue = [NSOperationQueue new];
    dispatch_queue_t underlying = dispatch_queue_create("pomme.ops", DISPATCH_QUEUE_SERIAL);
    queue.underlyingQueue = underlying;
    check(@"underlyingQueue", queue.underlyingQueue == underlying);

    NSMutableArray *out = [NSMutableArray new];
    for (NSArray *c in checks) [out addObject:[NSString stringWithFormat:@"%@=%@", c[0], c[1]]];
    UITextField *field = [[UITextField alloc] initWithFrame:CGRectMake(0, 100, 300, 30)];
    field.tag = 1;
    field.text = [out componentsJoinedByString:@" "];
    [self.view addSubview:field];
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
