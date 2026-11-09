// The navigation bar through a push and pop, for tests/navbar.rs: a red
// "Root" screen pushes a blue "Pushed" one at 0.5 s and pops it at 2 s, the
// bar showing (iPhone 17 Pro, as scripts/measure/transitions measured).
#import <UIKit/UIKit.h>

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)options {
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    UIViewController *root = [UIViewController new];
    root.view.backgroundColor = UIColor.redColor;
    root.title = @"Root";
    UINavigationController *nav = [[UINavigationController alloc] initWithRootViewController:root];
    self.window.rootViewController = nav;
    [self.window makeKeyAndVisible];
    UIViewController *pushed = [UIViewController new];
    pushed.view.backgroundColor = UIColor.blueColor;
    pushed.title = @"Pushed";
    [NSTimer scheduledTimerWithTimeInterval:0.5 repeats:NO block:^(NSTimer *t) { [nav pushViewController:pushed animated:YES]; }];
    [NSTimer scheduledTimerWithTimeInterval:2.0 repeats:NO block:^(NSTimer *t) { [nav popViewControllerAnimated:YES]; }];
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
