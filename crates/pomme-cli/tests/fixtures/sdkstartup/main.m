// What SDKs such as Firebase do at launch, for tests/sdk_startup.rs: each
// check is printed as name=0/1 in field 1.
#import <UIKit/UIKit.h>
#include <sys/sysctl.h>
#import <objc/runtime.h>
#import <SystemConfiguration/SystemConfiguration.h>

@protocol PommeProbe <NSObject>
@end

// A class with ivars of its own, for runtime-made subclasses' sizes.
@interface PommeSized : NSObject {
    char _padding[40];
}
@end
@implementation PommeSized
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

    // NSFileManager: attributes (and setting the protection class), moving,
    // copying and listing in the data container; the phone's storage.
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *logs = [dir stringByAppendingPathComponent:@"logs"];
    [fm createDirectoryAtPath:logs withIntermediateDirectories:YES attributes:nil error:NULL];
    NSString *a = [logs stringByAppendingPathComponent:@"a.txt"];
    [@"hello" writeToFile:a atomically:YES encoding:NSUTF8StringEncoding error:NULL];
    NSDictionary *attrs = [fm attributesOfItemAtPath:a error:NULL];
    check(@"attributes", [attrs[NSFileType] isEqual:NSFileTypeRegular] && [attrs[NSFileSize] intValue] == 5 && [attrs[NSFileModificationDate] isKindOfClass:NSDate.class] && [attrs[NSFilePosixPermissions] intValue] == 0644);
    BOOL set = [fm setAttributes:@{NSFileProtectionKey: NSFileProtectionNone} ofItemAtPath:a error:NULL];
    check(@"setAttributes", set && [[fm attributesOfItemAtPath:a error:NULL][NSFileProtectionKey] isEqual:NSFileProtectionNone]);
    NSError *missing = nil;
    check(@"setAttributesMissing", ![fm setAttributes:@{} ofItemAtPath:[logs stringByAppendingPathComponent:@"none"] error:&missing] && missing.code == NSFileNoSuchFileError);
    NSString *b = [logs stringByAppendingPathComponent:@"b.txt"];
    NSString *copied = [dir stringByAppendingPathComponent:@"logs-copy"];
    check(@"move", [fm moveItemAtPath:a toPath:b error:NULL] && ![fm fileExistsAtPath:a] && [fm fileExistsAtPath:b]);
    check(@"copy", [fm copyItemAtPath:logs toPath:copied error:NULL] && [[NSString stringWithContentsOfFile:[copied stringByAppendingPathComponent:@"b.txt"] encoding:NSUTF8StringEncoding error:NULL] isEqual:@"hello"]);
    NSError *exists = nil;
    check(@"copyOverExisting", ![fm copyItemAtPath:logs toPath:copied error:&exists] && exists.code == NSFileWriteFileExistsError);
    check(@"subpaths", [[fm subpathsAtPath:copied] isEqual:@[@"b.txt"]]);
    check(@"freeSpace", [[fm attributesOfFileSystemForPath:dir error:NULL][NSFileSystemFreeSize] longLongValue] > 0);
    check(@"writable", [fm isWritableFileAtPath:b] && ![fm isWritableFileAtPath:NSBundle.mainBundle.bundlePath] && [fm isReadableFileAtPath:NSBundle.mainBundle.bundlePath]);

    // NSMutableString's replaceOccurrences, as GTMSessionFetcher cleans a user agent.
    NSMutableString *agent = [@"My App (iPhone; iOS 26)" mutableCopy];
    NSUInteger replaced = [agent replaceOccurrencesOfString:@" " withString:@"_" options:0 range:NSMakeRange(0, agent.length)];
    check(@"replaceOccurrences", replaced == 4 && [agent isEqual:@"My_App_(iPhone;_iOS_26)"]);

    // NSUUID: random version-4 IDs, parsed and written back.
    NSUUID *u1 = [NSUUID UUID], *u2 = [[NSUUID alloc] initWithUUIDString:@"e621e1f8-c36c-495a-93fc-0c247a3e6e5f"];
    check(@"uuid", u1.UUIDString.length == 36 && ![u1 isEqual:[NSUUID UUID]] && [u2.UUIDString isEqual:@"E621E1F8-C36C-495A-93FC-0C247A3E6E5F"] && [u2 isEqual:[[NSUUID alloc] initWithUUIDString:u2.UUIDString]] && [[NSUUID alloc] initWithUUIDString:@"nope"] == nil);

    // Comparing and searching strings with options; paths from components.
    check(@"compare", [@"abc" caseInsensitiveCompare:@"ABC"] == NSOrderedSame && [@"file10" localizedStandardCompare:@"file9"] == NSOrderedDescending && [@"b" localizedCaseInsensitiveCompare:@"A"] == NSOrderedDescending && [@"a" compare:@"B" options:NSCaseInsensitiveSearch] == NSOrderedAscending);
    NSRange r1 = [@"Hello World" rangeOfString:@"world" options:NSCaseInsensitiveSearch];
    NSRange r2 = [@"a.b.c" rangeOfString:@"." options:NSBackwardsSearch];
    NSRange r3 = [@"abc123" rangeOfString:@"[0-9]+" options:NSRegularExpressionSearch];
    check(@"rangeOptions", r1.location == 6 && r1.length == 5 && r2.location == 3 && r3.location == 3 && r3.length == 3 && [@"abc" rangeOfString:@"b" options:NSAnchoredSearch].location == NSNotFound);
    check(@"pathWithComponents", [[NSString pathWithComponents:@[@"/", @"usr", @"lib"]] isEqual:@"/usr/lib"] && [[NSString pathWithComponents:@[@"a", @"b"]] isEqual:@"a/b"]);

    // NSUserDefaults domains: suites share by name, registration sits underneath, persistent domains by name.
    NSUserDefaults *suite = [[NSUserDefaults alloc] initWithSuiteName:@"group.pomme"];
    [suite setObject:@"v" forKey:@"k"];
    NSUserDefaults *same = [[NSUserDefaults alloc] initWithSuiteName:@"group.pomme"];
    [NSUserDefaults.standardUserDefaults registerDefaults:@{@"reg": @7}];
    check(@"defaultsSuite", [[same stringForKey:@"k"] isEqual:@"v"] && [NSUserDefaults.standardUserDefaults objectForKey:@"k"] == nil && [suite integerForKey:@"reg"] == 7);
    check(@"defaultsDomain", [[NSUserDefaults.standardUserDefaults persistentDomainForName:@"group.pomme"][@"k"] isEqual:@"v"] && [NSUserDefaults.standardUserDefaults persistentDomainForName:@"no.such.domain"] == nil);
    [NSUserDefaults.standardUserDefaults removePersistentDomainForName:@"group.pomme"];
    check(@"defaultsRemoveDomain", [same objectForKey:@"k"] == nil && [[NSUserDefaults alloc] initWithSuiteName:NSBundle.mainBundle.bundleIdentifier] == nil);

    // KVC in bulk.
    NSMutableDictionary *kvc = [NSMutableDictionary dictionary];
    [kvc setValuesForKeysWithDictionary:@{@"a": @1, @"b": @"two"}];
    check(@"kvcBulk", kvc.count == 2 && [kvc[@"b"] isEqual:@"two"]);

    // A runtime-made subclass is as big as its superclass (Firebase checks before swizzling isa).
    Class sub = objc_allocateClassPair(PommeSized.class, "PommeProbeSubclass", 0);
    objc_registerClassPair(sub);
    check(@"subclassSize", class_getInstanceSize(PommeSized.class) >= 48 && class_getInstanceSize(sub) == class_getInstanceSize(PommeSized.class));

    // Reachability: online over Wi-Fi.
    SCNetworkReachabilityRef reach = SCNetworkReachabilityCreateWithName(NULL, "firebase.googleapis.com");
    SCNetworkReachabilityFlags flags = 0;
    check(@"reachability", reach != NULL && SCNetworkReachabilityGetFlags(reach, &flags) && (flags & kSCNetworkReachabilityFlagsReachable) && !(flags & kSCNetworkReachabilityFlagsIsWWAN));
    if (reach) CFRelease(reach);

    // Temporary files: mkstemp makes and opens one with a fresh name.
    char tmpl[256];
    snprintf(tmpl, sizeof tmpl, "%s/pomme.XXXXXX", NSTemporaryDirectory().fileSystemRepresentation);
    int tfd = mkstemp(tmpl);
    check(@"mkstemp", tfd >= 0 && strstr(tmpl, "XXXXXX") == NULL && [fm fileExistsAtPath:@(tmpl)]);
    if (tfd >= 0) close(tfd);

    // Assertions (NSAssert) aren't checked here: pomme stops at an uncaught exception.
    check(@"assertionHandler", NSAssertionHandler.currentHandler != nil);

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
