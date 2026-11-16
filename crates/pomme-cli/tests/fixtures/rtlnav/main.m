// A navigation controller's push and pop in a right-to-left layout, for
// tests/rtl.rs: a red "Root" screen pushes a blue "Pushed" one at 0.5 s and
// pops it at 2 s. Every frame of each transition prints where the moving
// screen is (`PUSH t x`, `POP t x`: seconds since it began, the screen's
// left edge on screen); after the push the bar's views are listed (`BAR
// class x y width height text`, in the window), and the back swipe's edges.
// scripts/swiftui/metrics-simulator.sh records it (VARIANT=<language>).
#import <QuartzCore/QuartzCore.h>
#import <UIKit/UIKit.h>

static void dump(UIView *v, UIView *window) {
    CGRect r = [v convertRect:v.bounds toView:window];
    NSString *text = [v isKindOfClass:UILabel.class] ? ((UILabel *)v).text : @"";
    if (!v.hidden && v.alpha > 0.01 && r.size.width > 0) {
        printf("BAR %s %.2f %.2f %.2f %.2f %s\n", NSStringFromClass(v.class).UTF8String, r.origin.x, r.origin.y, r.size.width, r.size.height, (text ?: @"").UTF8String);
    }
    for (UIView *s in v.subviews) dump(s, window);
}

@interface Watcher : NSObject
@property(nonatomic, strong) UIView *view;
@property(nonatomic, copy) NSString *kind;
@property(nonatomic) CFTimeInterval start;
@property(nonatomic, strong) CADisplayLink *link;
@end

@implementation Watcher
- (void)watch:(UIView *)v kind:(NSString *)kind {
    self.view = v;
    self.kind = kind;
    self.start = CACurrentMediaTime();
    self.link = [CADisplayLink displayLinkWithTarget:self selector:@selector(tick)];
    [self.link addToRunLoop:NSRunLoop.mainRunLoop forMode:NSRunLoopCommonModes];
}
- (void)tick {
    CFTimeInterval t = CACurrentMediaTime() - self.start;
    CALayer *p = self.view.layer.presentationLayer ?: self.view.layer;
    CGRect r = [p convertRect:p.bounds toLayer:self.view.window.layer];
    printf("%s %.3f %.2f\n", self.kind.UTF8String, t, r.origin.x);
    fflush(stdout);
    if (t > 0.7) [self.link invalidate];
}
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@property(nonatomic, strong) Watcher *watcher;
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
    self.watcher = [Watcher new];
    [NSTimer scheduledTimerWithTimeInterval:0.5 repeats:NO block:^(NSTimer *t) {
        [nav pushViewController:pushed animated:YES];
        [self.watcher watch:pushed.view kind:@"PUSH"];
    }];
    [NSTimer scheduledTimerWithTimeInterval:1.6 repeats:NO block:^(NSTimer *t) {
        dump(nav.navigationBar, self.window);
        UIScreenEdgePanGestureRecognizer *g = (UIScreenEdgePanGestureRecognizer *)nav.interactivePopGestureRecognizer;
        printf("VALUE interactivePop.class %s\n", NSStringFromClass(g.class).UTF8String);
        if ([g respondsToSelector:@selector(edges)]) printf("VALUE interactivePop.edges %lu\n", (unsigned long)g.edges);
        fflush(stdout);
    }];
    [NSTimer scheduledTimerWithTimeInterval:2.0 repeats:NO block:^(NSTimer *t) {
        [nav popViewControllerAnimated:YES];
        [self.watcher watch:pushed.view kind:@"POP"];
    }];
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
