// NSURLComponents and NSURLQueryItem (tests/development_builds.rs): parsing a
// URL into parts, building one from parts and query items, and how each part
// is percent-encoded. React Native builds its bundle URL this way.
#import <UIKit/UIKit.h>

static const char *s(id v) { return v ? [[v description] UTF8String] : "(nil)"; }

static void report(void) {
    NSURLComponents *c = [NSURLComponents componentsWithURL:[NSURL URLWithString:@"http://user:pw@example.com:8081/a/b%20c?x=1&y=two%20words#frag"] resolvingAgainstBaseURL:NO];
    printf("VALUE parse.scheme %s\n", s(c.scheme));
    printf("VALUE parse.user %s\n", s(c.user));
    printf("VALUE parse.password %s\n", s(c.password));
    printf("VALUE parse.host %s\n", s(c.host));
    printf("VALUE parse.port %s\n", s(c.port));
    printf("VALUE parse.path %s\n", s(c.path));
    printf("VALUE parse.percentEncodedPath %s\n", s(c.percentEncodedPath));
    printf("VALUE parse.query %s\n", s(c.query));
    printf("VALUE parse.percentEncodedQuery %s\n", s(c.percentEncodedQuery));
    printf("VALUE parse.fragment %s\n", s(c.fragment));
    printf("VALUE parse.items %lu\n", (unsigned long)c.queryItems.count);
    printf("VALUE parse.item1 %s=%s\n", s(c.queryItems[1].name), s(c.queryItems[1].value));
    printf("VALUE parse.string %s\n", s(c.string));

    NSURLComponents *b = [NSURLComponents componentsWithURL:[NSURL URLWithString:@"http://localhost:8081"] resolvingAgainstBaseURL:NO];
    b.path = @"/index.bundle";
    b.queryItems = @[
        [NSURLQueryItem queryItemWithName:@"platform" value:@"ios"],
        [[NSURLQueryItem alloc] initWithName:@"a b" value:@"c&d=e"],
        [[NSURLQueryItem alloc] initWithName:@"plus" value:@"1+2"],
        [[NSURLQueryItem alloc] initWithName:@"slash" value:@"x/y?z:w@v"],
        [[NSURLQueryItem alloc] initWithName:@"novalue" value:nil],
        [[NSURLQueryItem alloc] initWithName:@"empty" value:@""],
        [[NSURLQueryItem alloc] initWithName:@"accent" value:@"é#%"],
    ];
    printf("VALUE build.url %s\n", s(b.URL.absoluteString));
    printf("VALUE build.percentEncodedQuery %s\n", s(b.percentEncodedQuery));
    printf("VALUE build.query %s\n", s(b.query));

    NSURLComponents *q = [NSURLComponents new];
    q.scheme = @"https";
    q.host = @"example.com";
    q.path = @"/p q";
    q.query = @"k=v w&x=y+z";
    printf("VALUE set.string %s\n", s(q.string));
    printf("VALUE set.percentEncodedQuery %s\n", s(q.percentEncodedQuery));
    NSArray *items = [NSURLComponents componentsWithString:@"http://h/?x=1&y=&z"].queryItems;
    NSMutableArray *parts = [NSMutableArray array];
    for (NSURLQueryItem *i in items) [parts addObject:[NSString stringWithFormat:@"%@:%@", i.name, i.value ?: @"(nil)"]];
    printf("VALUE items.read %s\n", [parts componentsJoinedByString:@","].UTF8String);
    printf("VALUE noQuery.items %s\n", s([NSURLComponents componentsWithString:@"http://h/p"].queryItems));
    printf("VALUE invalid %s\n", s([NSURLComponents componentsWithString:@"http://exa mple.com"]));
    NSURLQueryItem *qi = [NSURLQueryItem queryItemWithName:@"n" value:@"v"];
    printf("VALUE item.equal %d\n", [qi isEqual:[[NSURLQueryItem alloc] initWithName:@"n" value:@"v"]]);
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
