// Core Animation for tests/animation.rs. At 0.5 s a blue square (tag 2,
// 50 pt, from x 0) moves 200 pt right over 1 s with a linear UIView
// animation; its completion writes "done" in label 1. A standalone red layer
// fades out over 1 s with a CABasicAnimation (removed at the end, the model
// opacity stays 1); its delegate writes "stopped 1" in label 3 when it ends.
// A large activity indicator spins at (200, 600) throughout.
#import <QuartzCore/QuartzCore.h>
#import <UIKit/UIKit.h>

@interface ViewController : UIViewController <CAAnimationDelegate>
@property(nonatomic, strong) UILabel *stopped;
@end

@implementation ViewController
- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;
    UILabel *done = [[UILabel alloc] initWithFrame:CGRectMake(20, 400, 300, 30)];
    done.tag = 1;
    [self.view addSubview:done];
    self.stopped = [[UILabel alloc] initWithFrame:CGRectMake(20, 440, 300, 30)];
    self.stopped.tag = 3;
    [self.view addSubview:self.stopped];
    UIView *box = [[UIView alloc] initWithFrame:CGRectMake(0, 200, 50, 50)];
    box.backgroundColor = UIColor.blueColor;
    box.tag = 2;
    [self.view addSubview:box];
    UIActivityIndicatorView *spin = [[UIActivityIndicatorView alloc] initWithActivityIndicatorStyle:UIActivityIndicatorViewStyleLarge];
    spin.center = CGPointMake(200, 600);
    [self.view addSubview:spin];
    [spin startAnimating];
    CALayer *red = [CALayer layer];
    red.frame = CGRectMake(0, 300, 50, 50);
    red.backgroundColor = UIColor.redColor.CGColor;
    [self.view.layer addSublayer:red];

    [NSTimer scheduledTimerWithTimeInterval:0.5 repeats:NO block:^(NSTimer *t) {
        [UIView animateWithDuration:1.0 delay:0 options:UIViewAnimationOptionCurveLinear animations:^{
            box.center = CGPointMake(box.center.x + 200, box.center.y);
        } completion:^(BOOL finished) {
            done.text = finished ? @"done" : @"not finished";
        }];
        CABasicAnimation *fade = [CABasicAnimation animationWithKeyPath:@"opacity"];
        fade.fromValue = @1.0;
        fade.toValue = @0.0;
        fade.duration = 1.0;
        fade.delegate = self;
        [red addAnimation:fade forKey:@"fade"];
    }];
}

- (void)animationDidStop:(CAAnimation *)anim finished:(BOOL)flag {
    self.stopped.text = [NSString stringWithFormat:@"stopped %d", flag];
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
