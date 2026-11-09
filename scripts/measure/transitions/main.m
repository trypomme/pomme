// Measures UIKit's transitions in the iOS Simulator, for pomme to match:
// every display frame during each step it prints what's on screen (each
// view's presentation frame in window coordinates, opacity and transform),
// UIKit's own transition views included. Run with run.sh; the log is
// "M <step> <t> <depth> <class> <tag> <x> <y> <w> <h> <opacity> <a> <b> <c> <d> <tx> <ty>
// <background r g b a> <corner radius> <shadow opacity> <shadow radius> <shadow dx> <shadow dy> [label text]".
//
// Steps (seconds after launch): push 1, pop 3, present (default style) 5,
// dismiss 7, present full screen 9, dismiss 11, spinner 13–14, present
// cross-dissolve 15, dismiss 17, present a medium-detent sheet with a grabber
// 19, dismiss 21.
#import <QuartzCore/QuartzCore.h>
#import <UIKit/UIKit.h>

static NSString *step = nil;
static CFTimeInterval stepStart = 0;
static UIWindow *window = nil;

static UIViewController *colored(UIColor *c, NSInteger tag) {
    UIViewController *vc = [UIViewController new];
    vc.view.backgroundColor = c;
    vc.view.tag = tag;
    return vc;
}

static void dump(UIView *v, int depth) {
    CALayer *p = v.layer.presentationLayer ?: v.layer;
    CGRect r = [window.layer convertRect:p.bounds fromLayer:p];
    CGAffineTransform t = CATransform3DGetAffineTransform(p.transform);
    CGFloat rgba[4] = {0, 0, 0, 0};
    if (p.backgroundColor) {
        CGColorRef c = CGColorCreateCopyByMatchingToColorSpace(CGColorSpaceCreateWithName(kCGColorSpaceSRGB), kCGRenderingIntentDefault, p.backgroundColor, NULL);
        if (c && CGColorGetNumberOfComponents(c) == 4) memcpy(rgba, CGColorGetComponents(c), sizeof rgba);
    }
    NSString *text = [v isKindOfClass:UILabel.class] ? [((UILabel *)v).text stringByReplacingOccurrencesOfString:@" " withString:@"_"] : @"";
    printf("M %s %.4f %d %s %ld %.2f %.2f %.2f %.2f %.3f %.4f %.4f %.4f %.4f %.2f %.2f %.3f %.3f %.3f %.3f %.2f %.3f %.2f %.2f %.2f %s\n", step.UTF8String,
           CACurrentMediaTime() - stepStart, depth, NSStringFromClass(v.class).UTF8String, (long)v.tag,
           r.origin.x, r.origin.y, r.size.width, r.size.height, p.opacity, t.a, t.b, t.c, t.d, t.tx, t.ty,
           rgba[0], rgba[1], rgba[2], rgba[3], p.cornerRadius, p.shadowOpacity, p.shadowRadius, p.shadowOffset.width, p.shadowOffset.height, (text ?: @"").UTF8String);
    if (depth < 16) {
        for (UIView *s in v.subviews) dump(s, depth + 1);
    }
}

// The animations on a layer tree, once: "A <depth> <layer class> <key> <animation class> <keyPath>
// <duration> <repeatCount> <calculationMode> <values…>" (for the spinner).
static void animations(CALayer *l, int depth) {
    for (NSString *key in l.animationKeys) {
        CAAnimation *a = [l animationForKey:key];
        NSString *kp = [a isKindOfClass:CAPropertyAnimation.class] ? ((CAPropertyAnimation *)a).keyPath : @"-";
        NSString *mode = [a isKindOfClass:CAKeyframeAnimation.class] ? ((CAKeyframeAnimation *)a).calculationMode : @"-";
        NSArray *values = [a isKindOfClass:CAKeyframeAnimation.class] ? ((CAKeyframeAnimation *)a).values : nil;
        // The spinner's frames, as PNGs in the app's tmp directory ("spinner-<i>.png").
        if ([kp isEqualToString:@"contents"]) {
            [values enumerateObjectsUsingBlock:^(id img, NSUInteger i, BOOL *stop) {
                UIImage *u = [UIImage imageWithCGImage:(__bridge CGImageRef)img];
                NSString *path = [NSTemporaryDirectory() stringByAppendingPathComponent:[NSString stringWithFormat:@"spinner-%s-%02lu.png", getenv("SPIN") ?: "large", (unsigned long)i]];
                [UIImagePNGRepresentation(u) writeToFile:path atomically:YES];
            }];
            printf("F %s\n", NSTemporaryDirectory().UTF8String);
        }
        NSString *from = [a isKindOfClass:CABasicAnimation.class] ? [NSString stringWithFormat:@"%@>%@", ((CABasicAnimation *)a).fromValue, ((CABasicAnimation *)a).toValue] : @"";
        printf("A %d %s %s %s %s %.4f %.1f %s %lu %s %s\n", depth, NSStringFromClass(l.class).UTF8String, key.UTF8String,
               NSStringFromClass(a.class).UTF8String, kp.UTF8String, a.duration, a.repeatCount, mode.UTF8String,
               (unsigned long)values.count, [[values componentsJoinedByString:@","] stringByReplacingOccurrencesOfString:@"\n" withString:@" "].UTF8String, from.UTF8String);
    }
    printf("L %d %s %.1f %.1f %.1f %.1f op=%.2f\n", depth, NSStringFromClass(l.class).UTF8String, l.frame.origin.x, l.frame.origin.y, l.frame.size.width, l.frame.size.height, l.opacity);
    for (CALayer *s in l.sublayers) animations(s, depth + 1);
}

@interface Probe : NSObject
@end

@implementation Probe
- (void)tick:(CADisplayLink *)link {
    if (step && CACurrentMediaTime() - stepStart < 1.5) dump(window, 0);
    // Snapshots of the screen at moments in the push and pop (as shown: presentation values):
    // tmp/snap-<step>-<ms>.png, for looking at what the log can't say (the bar's titles).
    static NSArray *moments;
    static NSMutableSet *taken;
    if (!moments) { moments = @[@50, @100, @150, @200, @300, @600]; taken = [NSMutableSet set]; }
    if ([step isEqualToString:@"push"] || [step isEqualToString:@"pop"]) {
        int ms = (int)((CACurrentMediaTime() - stepStart) * 1000);
        for (NSNumber *m in moments) {
            NSString *key = [NSString stringWithFormat:@"%@-%@", step, m];
            if (ms >= m.intValue && ![taken containsObject:key]) {
                [taken addObject:key];
                UIGraphicsImageRenderer *r = [[UIGraphicsImageRenderer alloc] initWithSize:CGSizeMake(window.bounds.size.width, 140)];
                UIImage *img = [r imageWithActions:^(UIGraphicsImageRendererContext *c) {
                    [window drawViewHierarchyInRect:window.bounds afterScreenUpdates:NO];
                }];
                NSString *path = [NSTemporaryDirectory() stringByAppendingPathComponent:[NSString stringWithFormat:@"snap-%@-%03d.png", step, m.intValue]];
                [UIImagePNGRepresentation(img) writeToFile:path atomically:YES];
            }
        }
    }
}
@end

static void begin(NSString *name, void (^action)(void)) {
    step = name;
    stepStart = CACurrentMediaTime();
    action();
}

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@property(nonatomic, strong) Probe *probe;
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)options {
    setvbuf(stdout, NULL, _IOLBF, 0);
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    window = self.window;
    UIViewController *root = colored(UIColor.redColor, 1);
    UINavigationController *nav = [[UINavigationController alloc] initWithRootViewController:root];
    root.title = @"Root";
    self.window.rootViewController = nav;
    [self.window makeKeyAndVisible];
    UIActivityIndicatorView *spin = [[UIActivityIndicatorView alloc] initWithActivityIndicatorStyle:UIActivityIndicatorViewStyleLarge];
    spin.center = CGPointMake(60, 700);
    spin.tag = 9;
    [root.view addSubview:spin];
    [spin startAnimating];
    UIActivityIndicatorView *medium = [[UIActivityIndicatorView alloc] initWithActivityIndicatorStyle:UIActivityIndicatorViewStyleMedium];
    medium.center = CGPointMake(140, 700);
    medium.tag = 10;
    [root.view addSubview:medium];
    [medium startAnimating];

    self.probe = [Probe new];
    [[CADisplayLink displayLinkWithTarget:self.probe selector:@selector(tick:)] addToRunLoop:NSRunLoop.mainRunLoop forMode:NSRunLoopCommonModes];

    UIViewController *pushed = colored(UIColor.blueColor, 2);
    pushed.title = @"Pushed";
    __block UIViewController *sheet = colored(UIColor.greenColor, 3);
    __block UIViewController *full = colored(UIColor.yellowColor, 4);
    full.modalPresentationStyle = UIModalPresentationFullScreen;
    UIViewController *fade = colored(UIColor.cyanColor, 5);
    fade.modalPresentationStyle = UIModalPresentationFullScreen;
    fade.modalTransitionStyle = UIModalTransitionStyleCrossDissolve;
    UIViewController *mediumSheet = colored(UIColor.magentaColor, 6);
    mediumSheet.sheetPresentationController.detents = @[UISheetPresentationControllerDetent.mediumDetent];
    mediumSheet.sheetPresentationController.prefersGrabberVisible = YES;
    NSArray *plan = @[
        @[@1, @"push", ^{ [nav pushViewController:pushed animated:YES]; }],
        @[@3, @"pop", ^{ [nav popViewControllerAnimated:YES]; }],
        @[@5, @"present", ^{ [nav presentViewController:sheet animated:YES completion:nil]; }],
        @[@7, @"dismiss", ^{ [nav dismissViewControllerAnimated:YES completion:nil]; }],
        @[@9, @"presentFull", ^{ [nav presentViewController:full animated:YES completion:nil]; }],
        @[@11, @"dismissFull", ^{ [nav dismissViewControllerAnimated:YES completion:nil]; }],
        @[@13, @"spinner", ^{ animations(spin.layer, 0); setenv("SPIN", "medium", 1); animations(medium.layer, 0); }],
        @[@15, @"presentFade", ^{ [nav presentViewController:fade animated:YES completion:nil]; }],
        @[@17, @"dismissFade", ^{ [nav dismissViewControllerAnimated:YES completion:nil]; }],
        @[@19, @"presentMedium", ^{ [nav presentViewController:mediumSheet animated:YES completion:nil]; }],
        @[@21, @"dismissMedium", ^{ [nav dismissViewControllerAnimated:YES completion:nil]; }],
        @[@23, @"end", ^{ exit(0); }],
    ];
    for (NSArray *p in plan) {
        NSString *name = p[1];
        void (^action)(void) = p[2];
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)([p[0] doubleValue] * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
            begin(name, action);
        });
    }
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
