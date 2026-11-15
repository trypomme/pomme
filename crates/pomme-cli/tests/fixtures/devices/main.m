// Measures the device an app sees, for pomme's device table
// (crates/pomme-hle/src/device.rs): the screen, its scale and corner radius,
// the safe area, the status bar, the size classes, UIDevice's strings and the
// keyboard's height, in portrait and then landscape. Run on the Simulator by
// scripts/measure/devices/run.sh, once as an iPad app and once as an
// iPhone-only app (which iPadOS runs in its compatibility mode).
//
// It prints "METRIC <phase>.<name> <values…>", phase "portrait" or "landscape".
// Keyboards: "kb.predictive" for a default field, "kb.plain" for a field with
// autocorrection and spell checking off (the keyboard's frame in the window,
// height last).
#import <UIKit/UIKit.h>

static NSString *phase = @"portrait";
static UIWindow *window;
static NSString *pendingKeyboard;

static void metric(NSString *name, NSString *value) {
    printf("METRIC %s.%s %s\n", phase.UTF8String, name.UTF8String, value.UTF8String);
    fflush(stdout);
}

static NSString *insets(UIEdgeInsets i) { return [NSString stringWithFormat:@"%g %g %g %g", i.top, i.left, i.bottom, i.right]; }
static NSString *rect(CGRect r) { return [NSString stringWithFormat:@"%g %g %g %g", r.origin.x, r.origin.y, r.size.width, r.size.height]; }

static void report(void) {
    UIScreen *s = window.windowScene.screen;
    UITraitCollection *t = window.traitCollection;
    UIDevice *d = UIDevice.currentDevice;
    metric(@"screen", rect(s.bounds));
    metric(@"native", [NSString stringWithFormat:@"%@ %g %g", rect(s.nativeBounds), s.scale, s.nativeScale]);
    metric(@"window", rect(window.frame));
    metric(@"safe", insets(window.safeAreaInsets));
    metric(@"statusbar", rect(window.windowScene.statusBarManager.statusBarFrame));
    metric(@"orientation", [NSString stringWithFormat:@"%ld", (long)window.windowScene.interfaceOrientation]);
    metric(@"sizeclasses", [NSString stringWithFormat:@"%ld %ld", (long)t.horizontalSizeClass, (long)t.verticalSizeClass]);
    metric(@"idiom", [NSString stringWithFormat:@"%ld %ld", (long)d.userInterfaceIdiom, (long)t.userInterfaceIdiom]);
    metric(@"device", [NSString stringWithFormat:@"%@|%@|%@|%@", d.systemName, d.model, d.localizedModel,
                                                 NSProcessInfo.processInfo.environment[@"SIMULATOR_MODEL_IDENTIFIER"] ?: @"?"]);
    id corner = nil;
    @try { corner = [s valueForKey:@"_displayCornerRadius"]; } @catch (id e) {}
    metric(@"corner", [NSString stringWithFormat:@"%@", corner ?: @"?"]);
}

@interface Probe : UIViewController
@property(nonatomic, strong) UITextField *predictive, *plain;
@end

@implementation Probe
- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;
    self.predictive = [[UITextField alloc] initWithFrame:CGRectMake(20, 120, 240, 34)];
    self.plain = [[UITextField alloc] initWithFrame:CGRectMake(20, 170, 240, 34)];
    self.plain.autocorrectionType = UITextAutocorrectionTypeNo;
    self.plain.spellCheckingType = UITextSpellCheckingTypeNo;
    for (UITextField *f in @[ self.predictive, self.plain ]) {
        f.borderStyle = UITextBorderStyleRoundedRect;
        [self.view addSubview:f];
    }
    [NSNotificationCenter.defaultCenter addObserver:self selector:@selector(keyboard:) name:UIKeyboardDidShowNotification object:nil];
    [NSNotificationCenter.defaultCenter addObserver:self selector:@selector(keyboard:) name:UIKeyboardDidChangeFrameNotification object:nil];
}

- (void)keyboard:(NSNotification *)n {
    if (!pendingKeyboard) return;
    CGRect r = [n.userInfo[UIKeyboardFrameEndUserInfoKey] CGRectValue];
    r = [window convertRect:r fromCoordinateSpace:window.windowScene.screen.coordinateSpace];
    metric(pendingKeyboard, rect(r));
}

- (void)focus:(UITextField *)f as:(NSString *)name {
    pendingKeyboard = name;
    [f becomeFirstResponder];
}

- (void)dismiss {
    pendingKeyboard = nil;
    [self.view endEditing:YES];
}

// One phase: geometry, then each keyboard.
- (void)measureThen:(void (^)(void))next {
    report();
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 500 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{
        [self focus:self.predictive as:@"kb.predictive"];
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_SEC), dispatch_get_main_queue(), ^{
            [self dismiss];
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC), dispatch_get_main_queue(), ^{
                [self focus:self.plain as:@"kb.plain"];
                dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_SEC), dispatch_get_main_queue(), ^{
                    [self dismiss];
                    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC), dispatch_get_main_queue(), next);
                });
            });
        });
    });
}

- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    static BOOL started;
    if (started) return;
    started = YES;
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC), dispatch_get_main_queue(), ^{
        [self measureThen:^{
            phase = @"landscape";
            UIWindowSceneGeometryPreferencesIOS *p = [[UIWindowSceneGeometryPreferencesIOS alloc] initWithInterfaceOrientations:UIInterfaceOrientationMaskLandscapeRight];
            [window.windowScene requestGeometryUpdateWithPreferences:p errorHandler:^(NSError *e) {
                metric(@"rotate-error", e.localizedDescription ?: @"?");
            }];
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_SEC), dispatch_get_main_queue(), ^{
                [self measureThen:^{
                    printf("DONE\n");
                    fflush(stdout);
                }];
            });
        }];
    });
}
@end

@interface SceneDelegate : UIResponder <UIWindowSceneDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation SceneDelegate
- (void)scene:(UIScene *)scene willConnectToSession:(UISceneSession *)session options:(UISceneConnectionOptions *)options {
    window = [[UIWindow alloc] initWithWindowScene:(UIWindowScene *)scene];
    window.rootViewController = [Probe new];
    [window makeKeyAndVisible];
    self.window = window;
}
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@end

@implementation AppDelegate
- (UISceneConfiguration *)application:(UIApplication *)app configurationForConnectingSceneSession:(UISceneSession *)s options:(UISceneConnectionOptions *)o {
    UISceneConfiguration *c = [[UISceneConfiguration alloc] initWithName:@"Default" sessionRole:s.role];
    c.delegateClass = SceneDelegate.class;
    return c;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
