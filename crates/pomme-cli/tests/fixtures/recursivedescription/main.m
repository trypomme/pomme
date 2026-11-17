// -[UIView recursiveDescription] (UIKit's, private): the view tree as text,
// which React Native's developer menu searches for "RCTView" on a shake
// (tests/development_builds.rs). Addresses are replaced with 0x?, one
// VALUE line per line of the description.
#import <UIKit/UIKit.h>

@interface RCTViewLike : UIView
@end
@implementation RCTViewLike
@end

static void report(UIView *v, NSString *name) {
    NSString *d = [v valueForKey:@"recursiveDescription"];
    NSRegularExpression *hex = [NSRegularExpression regularExpressionWithPattern:@"0x[0-9a-f]+" options:0 error:nil];
    d = [hex stringByReplacingMatchesInString:d options:0 range:NSMakeRange(0, d.length) withTemplate:@"0x?"];
    NSArray *lines = [d componentsSeparatedByString:@"\n"];
    printf("VALUE %s.lines %lu\n", name.UTF8String, (unsigned long)lines.count);
    for (NSUInteger i = 0; i < lines.count; i++) printf("VALUE %s.%02lu %s\n", name.UTF8String, (unsigned long)i, [lines[i] UTF8String]);
    printf("VALUE %s.hasRCTView %d\n", name.UTF8String, [d containsString:@"RCTView"]);
}

@interface VC : UIViewController
@end
@implementation VC
- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;
    RCTViewLike *box = [[RCTViewLike alloc] initWithFrame:CGRectMake(10, 20, 100, 50)];
    UIView *inner = [[UIView alloc] initWithFrame:CGRectMake(1.5, 2, 30, 40)];
    inner.hidden = YES;
    inner.alpha = 0.5;
    [box addSubview:inner];
    [self.view addSubview:box];
}
- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    report(self.view.subviews.firstObject, @"box");
    fflush(stdout);
}
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end
@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)options {
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    self.window.rootViewController = [VC new];
    [self.window makeKeyAndVisible];
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
