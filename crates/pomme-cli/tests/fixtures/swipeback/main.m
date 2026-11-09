// The back swipe, for tests/swipeback.rs: a red root screen pushes a blue
// one at 0.5 s; the test swipes from the left edge. Label 1 on the root
// screen counts its viewDidAppear calls.
#import <UIKit/UIKit.h>

@interface Root : UIViewController
@property(nonatomic, strong) UILabel *label;
@property(nonatomic) int appeared;
@end

@implementation Root
- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.redColor;
    self.label = [[UILabel alloc] initWithFrame:CGRectMake(20, 600, 300, 30)];
    self.label.tag = 1;
    [self.view addSubview:self.label];
}
- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    self.appeared += 1;
    self.label.text = [NSString stringWithFormat:@"appeared %d", self.appeared];
}
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)options {
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    Root *root = [Root new];
    root.title = @"Root";
    UINavigationController *nav = [[UINavigationController alloc] initWithRootViewController:root];
    self.window.rootViewController = nav;
    [self.window makeKeyAndVisible];
    UIViewController *pushed = [UIViewController new];
    pushed.view.backgroundColor = UIColor.blueColor;
    pushed.title = @"Pushed";
    [NSTimer scheduledTimerWithTimeInterval:0.5 repeats:NO block:^(NSTimer *t) { [nav pushViewController:pushed animated:YES]; }];
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
