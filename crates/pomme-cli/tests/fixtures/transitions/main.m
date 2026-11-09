// UIKit's transitions, for tests/transitions.rs: a navigation controller over
// a red screen pushes a blue one at 0.5 s and pops it at 2 s; at 3.5 s a
// green controller is presented (the default style, a page sheet) and
// dismissed at 5 s; at 6.5 s a yellow one is presented full screen and
// dismissed at 8 s. Label 1 (on the red screen) lists the completions.
#import <UIKit/UIKit.h>

static UIViewController *colored(UIColor *c) {
    UIViewController *vc = [UIViewController new];
    vc.view.backgroundColor = c;
    return vc;
}

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)options {
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    UIViewController *root = colored(UIColor.redColor);
    UINavigationController *nav = [[UINavigationController alloc] initWithRootViewController:root];
    nav.navigationBarHidden = YES;
    self.window.rootViewController = nav;
    [self.window makeKeyAndVisible];
    UILabel *log = [[UILabel alloc] initWithFrame:CGRectMake(20, 600, 350, 30)];
    log.tag = 1;
    log.text = @"";
    [root.view addSubview:log];
    void (^note)(NSString *) = ^(NSString *s) { log.text = [log.text stringByAppendingFormat:@"%@ ", s]; };
    UIViewController *blue = colored(UIColor.blueColor);
    UIViewController *green = colored(UIColor.greenColor);
    UIViewController *yellow = colored(UIColor.yellowColor);
    yellow.modalPresentationStyle = UIModalPresentationFullScreen;
    NSArray *plan = @[
        @[@0.5, ^{ [nav pushViewController:blue animated:YES]; }],
        @[@2.0, ^{ [nav popViewControllerAnimated:YES]; }],
        @[@3.5, ^{ [nav presentViewController:green animated:YES completion:^{ note(@"presented"); }]; }],
        @[@5.0, ^{ [nav dismissViewControllerAnimated:YES completion:^{ note(@"dismissed"); }]; }],
        @[@6.5, ^{ [nav presentViewController:yellow animated:YES completion:^{ note(@"full"); }]; }],
        @[@8.0, ^{ [nav dismissViewControllerAnimated:YES completion:^{ note(@"back"); }]; }],
    ];
    for (NSArray *p in plan) {
        void (^action)(void) = p[1];
        [NSTimer scheduledTimerWithTimeInterval:[p[0] doubleValue] repeats:NO block:^(NSTimer *t) { action(); }];
    }
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
