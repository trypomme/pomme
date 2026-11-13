// What SDKs such as Firebase do at launch, for tests/sdk_startup.rs: each
// check is printed as name=0/1 in field 1.
#import <UIKit/UIKit.h>
#include <sys/sysctl.h>
#import <objc/runtime.h>
#import <SystemConfiguration/SystemConfiguration.h>
#include <sys/mount.h>
#include <malloc/malloc.h>
#include <sqlite3.h>

@protocol PommeProbe <NSObject>
@end

// A class with ivars of its own, for runtime-made subclasses' sizes.
@interface PommeSized : NSObject {
    char _padding[40];
}
@end
@implementation PommeSized
@end

// A class that archives itself (NSSecureCoding), for keyed archives.
@interface PommeNote : NSObject <NSSecureCoding>
@property (copy) NSString *title;
@property NSInteger count;
@property double score;
@property BOOL done;
@property (strong) NSArray *tags;
@end
@implementation PommeNote
+ (BOOL)supportsSecureCoding { return YES; }
- (void)encodeWithCoder:(NSCoder *)c {
    [c encodeObject:self.title forKey:@"title"];
    [c encodeInteger:self.count forKey:@"count"];
    [c encodeDouble:self.score forKey:@"score"];
    [c encodeBool:self.done forKey:@"done"];
    [c encodeObject:self.tags forKey:@"tags"];
}
- (instancetype)initWithCoder:(NSCoder *)c {
    if ((self = [super init])) {
        _title = [c decodeObjectOfClass:NSString.class forKey:@"title"];
        _count = [c decodeIntegerForKey:@"count"];
        _score = [c decodeDoubleForKey:@"score"];
        _done = [c decodeBoolForKey:@"done"];
        _tags = [c decodeObjectOfClasses:[NSSet setWithObjects:NSArray.class, NSString.class, nil] forKey:@"tags"];
    }
    return self;
}
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

    // Initializers sent to NSMutableString make mutable strings (Analytics appends a WHERE clause).
    NSMutableString *sql = [[NSMutableString alloc] initWithFormat:@"SELECT count(1) FROM %@", @"events"];
    [sql appendFormat:@" WHERE %@ = ?", @"name"];
    NSMutableString *utf = [[NSMutableString alloc] initWithUTF8String:"ab"];
    [utf appendString:@"c"];
    NSMutableString *bytes = [[NSMutableString alloc] initWithData:[@"x" dataUsingEncoding:NSUTF8StringEncoding] encoding:NSUTF8StringEncoding];
    [bytes appendString:@"y"];
    check(@"mutableInit", [sql isEqual:@"SELECT count(1) FROM events WHERE name = ?"] && [utf isEqual:@"abc"] && [bytes isEqual:@"xy"]);

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

    // Number and date formatters (CFNumberFormatter / CFDateFormatter, ICU).
    NSNumberFormatter *nf = [NSNumberFormatter new];
    nf.numberStyle = NSNumberFormatterDecimalStyle;
    nf.locale = [NSLocale localeWithLocaleIdentifier:@"en_US"];
    NSNumberFormatter *pct = [NSNumberFormatter new];
    pct.numberStyle = NSNumberFormatterPercentStyle;
    check(@"numberFormatter", [[nf stringFromNumber:@1234567.5] isEqual:@"1,234,567.5"] && [[nf numberFromString:@"1,234"] isEqual:@1234] && [nf numberFromString:@"12abc"] == nil && [[pct stringFromNumber:@0.25] isEqual:@"25%"]);
    NSDateFormatter *df = [NSDateFormatter new];
    df.locale = [NSLocale localeWithLocaleIdentifier:@"en_US_POSIX"];
    df.timeZone = [NSTimeZone timeZoneForSecondsFromGMT:0];
    df.dateFormat = @"yyyy-MM-dd'T'HH:mm:ss";
    NSDate *when = [NSDate dateWithTimeIntervalSince1970:1767225600 + 3661]; // 2026-01-01 01:01:01 GMT
    NSDateFormatter *sydney = [NSDateFormatter new];
    sydney.locale = df.locale;
    sydney.dateFormat = @"HH:mm";
    sydney.timeZone = [NSTimeZone timeZoneWithName:@"Australia/Sydney"];
    check(@"dateFormatter", [[df stringFromDate:when] isEqual:@"2026-01-01T01:01:01"] && [[df dateFromString:@"2026-01-01T01:01:01"] isEqual:when] && [df dateFromString:@"nope"] == nil && [[sydney stringFromDate:when] isEqual:@"12:01"]);

    // Keyed archives: Foundation's classes and one that archives itself, there and back.
    PommeNote *note = [PommeNote new];
    note.title = @"hello"; note.count = 3; note.score = 2.5; note.done = YES; note.tags = @[@"a", @"b"];
    NSDictionary *root = @{@"note": note, @"n": @42, @"when": when, @"set": [NSSet setWithObject:@"x"], @"null": NSNull.null};
    NSData *archive = [NSKeyedArchiver archivedDataWithRootObject:root requiringSecureCoding:YES error:NULL];
    NSDictionary *unarchived = [NSKeyedUnarchiver unarchivedObjectOfClasses:[NSSet setWithObjects:NSDictionary.class, PommeNote.class, nil] fromData:archive error:NULL];
    PommeNote *restored = unarchived[@"note"];
    check(@"keyedArchive", archive.length > 100 && [restored isKindOfClass:PommeNote.class] && [restored.title isEqual:@"hello"] && restored.count == 3 && restored.score == 2.5 && restored.done && [restored.tags isEqual:@[@"a", @"b"]] && [unarchived[@"n"] isEqual:@42] && [unarchived[@"when"] isEqual:when] && [unarchived[@"set"] containsObject:@"x"] && unarchived[@"null"] == NSNull.null);
    NSPropertyListFormat archiveFormat = 0;
    NSDictionary *raw = [NSPropertyListSerialization propertyListWithData:archive options:0 format:&archiveFormat error:NULL];
    check(@"keyedArchiveFormat", archiveFormat == NSPropertyListBinaryFormat_v1_0 && [raw[@"$archiver"] isEqual:@"NSKeyedArchiver"] && [raw[@"$objects"][0] isEqual:@"$null"]);
    check(@"keyedArchiveBadData", [NSKeyedUnarchiver unarchivedObjectOfClass:NSDictionary.class fromData:[@"x" dataUsingEncoding:NSUTF8StringEncoding] error:NULL] == nil);

    // Sets, character sets, file systems, the malloc zone, background refresh.
    NSSet *s123 = [NSSet setWithArray:@[@1, @2, @3]], *s12 = [NSSet setWithArray:@[@1, @2]];
    NSMutableSet *ms = [s123 mutableCopy];
    [ms minusSet:s12];
    check(@"setAlgebra", [s12 isSubsetOfSet:s123] && ![s123 isSubsetOfSet:s12] && [s12 intersectsSet:s123] && [ms isEqualToSet:[NSSet setWithObject:@3]] && [[s12 setByAddingObject:@9] count] == 3);
    check(@"charsets", [NSCharacterSet.nonBaseCharacterSet characterIsMember:0x0301] && ![NSCharacterSet.URLQueryAllowedCharacterSet characterIsMember:' '] && [[@"a b&c" stringByAddingPercentEncodingWithAllowedCharacters:NSCharacterSet.URLQueryAllowedCharacterSet] isEqual:@"a%20b&c"] && [@"a%20b%E2%9C%93".stringByRemovingPercentEncoding isEqual:@"a b\u2713"] && @"100%".stringByRemovingPercentEncoding == nil);
    struct statfs sfs;
    check(@"statfs", statfs(NSTemporaryDirectory().fileSystemRepresentation, &sfs) == 0 && strcmp(sfs.f_fstypename, "apfs") == 0 && sfs.f_bavail > 0);
    malloc_zone_t *zone = malloc_default_zone();
    void *zp = malloc_zone_malloc(zone, 40);
    check(@"mallocZone", zone != NULL && zone->size(zone, zp) >= 40 && zone->zone_name != NULL);
    malloc_zone_free(zone, zp);
    check(@"backgroundRefresh", UIApplication.sharedApplication.backgroundRefreshStatus == UIBackgroundRefreshStatusAvailable);

    // A dispatch block can be waited on.
    __block int ran = 0;
    dispatch_block_t work = dispatch_block_create(0, ^{ ran = 1; });
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_DEFAULT, 0), work);
    check(@"dispatchBlockWait", dispatch_block_wait(work, dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_SEC)) == 0 && ran == 1);

    // SQLite on a file in the data container: tables, binding, a transaction.
    sqlite3 *db = NULL;
    NSString *dbPath = [NSTemporaryDirectory() stringByAppendingPathComponent:@"probe.sqlite"];
    BOOL sqlOK = sqlite3_open_v2(dbPath.fileSystemRepresentation, &db, SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE, NULL) == SQLITE_OK;
    sqlOK = sqlOK && sqlite3_exec(db, "CREATE TABLE t(k TEXT PRIMARY KEY, v INTEGER); BEGIN; INSERT INTO t VALUES('a', 1); INSERT INTO t VALUES('b', 2); COMMIT;", NULL, NULL, NULL) == SQLITE_OK;
    sqlite3_stmt *st = NULL;
    sqlOK = sqlOK && sqlite3_prepare_v2(db, "SELECT sum(v) FROM t WHERE k >= ?", -1, &st, NULL) == SQLITE_OK && sqlite3_bind_text(st, 1, "a", -1, SQLITE_STATIC) == SQLITE_OK && sqlite3_step(st) == SQLITE_ROW && sqlite3_column_int(st, 0) == 3;
    sqlite3_finalize(st);
    sqlite3_close(db);
    check(@"sqlite", sqlOK && [fm fileExistsAtPath:dbPath]);

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
