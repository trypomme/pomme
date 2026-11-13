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
#include <dlfcn.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/mman.h>
#include <mach-o/arch.h>
#include <mach-o/ldsyms.h>
#include <mach-o/loader.h>
#include <mach/mach.h>
#include <sys/syscall.h>
#include <errno.h>
#include <objc/message.h>

// The exclusive monitor across a thread switch: a load-exclusive, then a
// yield to a thread that writes the same word. The kernel clears the monitor
// on a context switch, so the store-exclusive after it fails (and lock-free
// code retries) instead of overwriting the other thread's write.
static unsigned long word;

static void *writer(void *arg) {
    word = 42;
    return NULL;
}

// A thread on a stack the caller made, that spins until told to stop.
static volatile int spinStop;
static volatile unsigned long spins;
static void *spinner(void *arg) {
    while (!spinStop) spins++;
    return arg;
}


// Collections that aren't CoreFoundation's own (as Swift's bridged arrays,
// dictionaries and strings aren't), for C code that reads them through CF.
@interface PommeArray : NSArray
@end
@implementation PommeArray
- (NSUInteger)count { return 3; }
- (id)objectAtIndex:(NSUInteger)i { return @[@"a", @"b", @"c"][i]; }
@end

@interface PommeDictionary : NSDictionary
@end
@implementation PommeDictionary
- (NSUInteger)count { return 2; }
- (id)objectForKey:(id)key { return @{@"one": @1, @"two": @2}[key]; }
- (NSArray *)allKeys { return @[@"one", @"two"]; }
- (NSEnumerator *)keyEnumerator { return self.allKeys.objectEnumerator; }
@end

@interface PommeString : NSString
@end
@implementation PommeString
- (NSUInteger)length { return 5; }
- (unichar)characterAtIndex:(NSUInteger)i { return "pomme"[i]; }
@end

static void countApplied(const void *value, void *context) { *(int *)context += 1; }
static void sumApplied(const void *key, const void *value, void *context) { *(long *)context += [(__bridge NSNumber *)value longValue]; }

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

    // Finding and reading its own executable, as crash reporters do: the
    // header's symbol, dladdr for its path, then the file mapped in.
    void *header = dlsym(RTLD_MAIN_ONLY, MH_EXECUTE_SYM);
    Dl_info info = {0};
    int found = header ? dladdr(header, &info) : 0;
    int fd = found ? open(info.dli_fname, O_RDONLY) : -1;
    NSDictionary *attrs = found ? [NSFileManager.defaultManager attributesOfItemAtPath:@(info.dli_fname) error:NULL] : nil;
    void *mapped = fd >= 0 ? mmap(NULL, [attrs fileSize], PROT_READ, MAP_FILE | MAP_SHARED, fd, 0) : MAP_FAILED;
    const NXArchInfo *arch = NXGetLocalArchInfo();
    check(@"selfImage", header && info.dli_fbase == header && fd >= 0 && [attrs[NSFileType] isEqual:NSFileTypeRegular] && mapped != MAP_FAILED &&
                            ((struct mach_header_64 *)mapped)->magic == MH_MAGIC_64 && arch && arch->cputype == CPU_TYPE_ARM64);
    if (fd >= 0) close(fd);

    // NSOrderedSet: distinct objects in order.
    NSMutableOrderedSet *os = [NSMutableOrderedSet orderedSetWithArray:@[@"b", @"a", @"b", @"c"]];
    [os addObject:@"a"];
    [os insertObject:@"z" atIndex:0];
    [os removeObject:@"c"];
    NSOrderedSet *frozen = [os copy];
    NSMutableArray *osSeen = [NSMutableArray new];
    for (NSString *x in frozen) [osSeen addObject:x];
    check(@"orderedSet", [os.array isEqual:(@[@"z", @"b", @"a"])] && [os indexOfObject:@"a"] == 2 && [os containsObject:@"b"] && ![os containsObject:@"c"] &&
                             [osSeen isEqual:os.array] && [frozen isEqualToOrderedSet:os] && ![frozen isKindOfClass:NSMutableOrderedSet.class] && [os.set count] == 3);

    // Threads: a caller-made stack, suspend and resume, registers, exception ports.
    pthread_attr_t attr;
    pthread_attr_init(&attr);
    size_t stackSize = 64 * 1024;
    void *stack = NULL;
    posix_memalign(&stack, 16384, stackSize);
    pthread_attr_setstack(&attr, stack, stackSize);
    pthread_t spin;
    pthread_create(&spin, &attr, spinner, NULL);
    mach_port_t port = pthread_mach_thread_np(spin);
    sched_yield();
    BOOL suspended = thread_suspend(port) == KERN_SUCCESS;
    unsigned long before = spins;
    sched_yield();
    BOOL stayed = spins == before;
    arm_thread_state64_t state;
    mach_msg_type_number_t count = ARM_THREAD_STATE64_COUNT;
    BOOL gotState = thread_get_state(port, ARM_THREAD_STATE64, (thread_state_t)&state, &count) == KERN_SUCCESS && count == ARM_THREAD_STATE64_COUNT;
    uint64_t sp = arm_thread_state64_get_sp(state);
    BOOL onStack = sp > (uint64_t)stack && sp <= (uint64_t)stack + stackSize;
    thread_resume(port);
    spinStop = 1;
    pthread_join(spin, NULL);
    check(@"threads", suspended && stayed && gotState && onStack && pthread_from_mach_thread_np(port) == spin && mach_thread_self() == pthread_mach_thread_np(pthread_self()));

    mach_port_t excPort;
    mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_RECEIVE, &excPort);
    exception_mask_t masks[EXC_TYPES_COUNT];
    mach_port_t ports[EXC_TYPES_COUNT];
    exception_behavior_t behaviors[EXC_TYPES_COUNT];
    thread_state_flavor_t flavors[EXC_TYPES_COUNT];
    mach_msg_type_number_t excCount = EXC_TYPES_COUNT;
    kern_return_t swapped = task_swap_exception_ports(mach_task_self(), EXC_MASK_BAD_ACCESS | EXC_MASK_CRASH, excPort, EXCEPTION_DEFAULT, THREAD_STATE_NONE, masks, &excCount, ports, behaviors, flavors);
    BOOL none = swapped == KERN_SUCCESS && excCount == 0;
    excCount = EXC_TYPES_COUNT;
    task_get_exception_ports(mach_task_self(), EXC_MASK_BAD_ACCESS | EXC_MASK_CRASH, masks, &excCount, ports, behaviors, flavors);
    check(@"exceptionPorts", none && excCount == 1 && ports[0] == excPort && masks[0] == (EXC_MASK_BAD_ACCESS | EXC_MASK_CRASH));

    // Files: removing what isn't there; gettid's credentials.
    NSError *removeError = nil;
    BOOL removed = [NSFileManager.defaultManager removeItemAtPath:[NSTemporaryDirectory() stringByAppendingPathComponent:@"nothing-here"] error:&removeError];
    errno = 0;
    uid_t uid;
    gid_t gid;
    long tid = syscall(SYS_gettid, &uid, &gid);
    check(@"errors", !removed && [removeError.domain isEqual:NSCocoaErrorDomain] && removeError.code == NSFileNoSuchFileError && tid == -1 && errno == ESRCH);

    // Toll-free bridging for objects that aren't CF's own, and NSNull's one instance.
    CFArrayRef ca = (__bridge CFArrayRef)[PommeArray new];
    int applied = 0;
    CFArrayApplyFunction(ca, CFRangeMake(0, 3), countApplied, &applied);
    CFDictionaryRef cd = (__bridge CFDictionaryRef)[PommeDictionary new];
    long sum = 0;
    CFDictionaryApplyFunction(cd, sumApplied, &sum);
    CFStringRef cs = (__bridge CFStringRef)[PommeString new];
    char cbuf[16] = {0};
    BOOL bridged = CFGetTypeID(ca) == CFArrayGetTypeID() && CFArrayGetCount(ca) == 3 && [(__bridge NSString *)CFArrayGetValueAtIndex(ca, 1) isEqual:@"b"] && applied == 3 &&
                   CFGetTypeID(cd) == CFDictionaryGetTypeID() && CFDictionaryGetCount(cd) == 2 && [(__bridge NSNumber *)CFDictionaryGetValue(cd, @"two") intValue] == 2 && sum == 3 &&
                   CFGetTypeID(cs) == CFStringGetTypeID() && CFStringGetLength(cs) == 5 && CFStringGetCString(cs, cbuf, sizeof cbuf, kCFStringEncodingUTF8) && strcmp(cbuf, "pomme") == 0;
    BOOL oneNull = [[NSNull alloc] init] == [NSNull null] && (__bridge CFNullRef)[NSNull null] == kCFNull && [NSNull new] == [NSNull null];
    check(@"tollFree", bridged && oneNull && ((BOOL (*)(id, SEL))objc_msgSend)(@"x", sel_registerName("isNSString__")) &&
                           !((BOOL (*)(id, SEL))objc_msgSend)([NSNull null], sel_registerName("isNSString__")) && ((BOOL (*)(id, SEL))objc_msgSend)(@[], sel_registerName("isNSArray__")));

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

    // A download task: the body as a file, with the headers in canonical form.
    NSURL *url = [NSURL URLWithString:@"https://pomme.test/hello.txt"];
    [[NSURLSession.sharedSession downloadTaskWithURL:url completionHandler:^(NSURL *location, NSURLResponse *response, NSError *error) {
        NSString *body = [NSString stringWithContentsOfURL:location encoding:NSUTF8StringEncoding error:NULL];
        NSHTTPURLResponse *http = (NSHTTPURLResponse *)response;
        BOOL ok = !error && [body isEqual:@"hello"] && http.statusCode == 200 && [http.allHeaderFields[@"Content-Type"] isEqual:@"text/plain"] &&
                  [http.allHeaderFields[@"X-Pomme-Id"] isEqual:@"7"];
        NSString *path = location.path;
        dispatch_async(dispatch_get_main_queue(), ^{
            // The file is gone once the handler returns.
            BOOL gone = ![NSFileManager.defaultManager fileExistsAtPath:path];
            field.text = [field.text stringByAppendingFormat:@" download=%d", ok && gone];
        });
    }] resume];
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
