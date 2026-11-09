// A known hot spot for tests/profile.rs: viewDidLoad calls spin(), which
// loops for a few million instructions, and a little of everything else.
#import <UIKit/UIKit.h>

__attribute__((noinline)) unsigned spin(unsigned n) {
    volatile unsigned acc = 0;
    for (unsigned i = 0; i < n; i++) acc += i * 2654435761u;
    return acc;
}

@interface ViewController : UIViewController
@end

@implementation ViewController
- (void)viewDidLoad {
    [super viewDidLoad];
    UILabel *l = [[UILabel alloc] initWithFrame:CGRectMake(20, 100, 300, 30)];
    l.tag = 1;
    l.text = [NSString stringWithFormat:@"%u", spin(2000000)];
    [self.view addSubview:l];
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
