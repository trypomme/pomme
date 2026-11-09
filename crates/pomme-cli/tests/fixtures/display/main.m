// When drawRect: runs, for tests/display.rs: a view counts its draws into
// a label (tag 1). It should draw when it first shows, after setNeedsDisplay
// (at 2 s) and after its size changes (at 3 s), and not for every frame.
#import <UIKit/UIKit.h>

@interface Counter : UIView
@property(nonatomic, weak) UILabel *label;
@property(nonatomic) int draws;
@end

@implementation Counter
- (void)drawRect:(CGRect)rect {
    self.draws += 1;
    [[UIColor systemBlueColor] setFill];
    UIRectFill(rect);
    self.label.text = [NSString stringWithFormat:@"draws=%d", self.draws];
}
@end

@interface ViewController : UIViewController
@end

@implementation ViewController
- (void)viewDidLoad {
    [super viewDidLoad];
    UILabel *label = [[UILabel alloc] initWithFrame:CGRectMake(20, 100, 300, 30)];
    label.tag = 1;
    [self.view addSubview:label];
    Counter *c = [[Counter alloc] initWithFrame:CGRectMake(20, 150, 100, 100)];
    c.label = label;
    [self.view addSubview:c];
    [NSTimer scheduledTimerWithTimeInterval:2 repeats:NO block:^(NSTimer *t) { [c setNeedsDisplay]; }];
    [NSTimer scheduledTimerWithTimeInterval:3 repeats:NO block:^(NSTimer *t) { c.frame = CGRectMake(20, 150, 150, 100); }];
    // Something ticking that changes nothing on screen, like a display link.
    [NSTimer scheduledTimerWithTimeInterval:1.0 / 60 repeats:YES block:^(NSTimer *t) { [self.view setNeedsLayout]; }];
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
