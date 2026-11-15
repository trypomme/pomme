// The coverage report's probes (tests/coverage.rs): an app that reaches one
// of each kind of gap, then carries on.
//
// - pomme_coverage_probe(): a C function no library has. It's linked with
//   `-U` (ldflags), so it binds by dynamic lookup, and no version of pomme
//   will ever implement it: a missing function, forever.
// - -pommeCoverageProbe: a selector no class has: a missing method.
// - -setPommeCoverageProbe: on a UIView: pomme answers UIView's unknown
//   setters generically (stores them) and nothing reads the value.
//
// With the recorder keeping going, each miss answers 0 and the app shows
// "after" in the field tagged 100. On iOS the first two would crash it.

#import <UIKit/UIKit.h>

extern int pomme_coverage_probe(int);

// Declared so the compiler sends them directly (objc_msgSend), as app code
// does. (Not through -performSelector:, which pomme delivers host-side.)
@interface NSObject (CoverageProbes)
- (id)pommeCoverageProbe;
@end

@interface UIView (CoverageProbes)
- (void)setPommeCoverageProbe:(id)value;
@end

@interface ViewController : UIViewController
@end

@implementation ViewController
- (void)viewDidLoad {
    [super viewDidLoad];
    int r = pomme_coverage_probe(7) + pomme_coverage_probe(8);
    id m = [[NSObject new] pommeCoverageProbe];
    UIView *v = [UIView new];
    [v setPommeCoverageProbe:@5];
    UITextField *f = [[UITextField alloc] initWithFrame:CGRectMake(20, 80, 350, 40)];
    f.tag = 100;
    f.text = [NSString stringWithFormat:@"after r=%d m=%@", r, m ? @"set" : @"nil"];
    [self.view addSubview:f];
}
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)application didFinishLaunchingWithOptions:(NSDictionary *)options {
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    self.window.rootViewController = [ViewController new];
    [self.window makeKeyAndVisible];
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass([AppDelegate class]));
    }
}
