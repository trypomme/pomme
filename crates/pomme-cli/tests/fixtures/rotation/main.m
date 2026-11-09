// A UIKit app that logs what it hears as the phone turns, for
// tests/rotation.rs. The log is the text of field 1; the geometry after the
// last turn is in field 2. The "Lock" button locks it to portrait.
#import <UIKit/UIKit.h>

static NSMutableArray<NSString *> *events;
static BOOL logging;

static void note(NSString *s) {
    if (logging) [events addObject:s];
}

static NSString *size(CGSize s) { return [NSString stringWithFormat:@"%gx%g", s.width, s.height]; }
static NSString *insets(UIEdgeInsets i) { return [NSString stringWithFormat:@"%g,%g,%g,%g", i.top, i.left, i.bottom, i.right]; }

@interface SafeView : UIView
@end
@implementation SafeView
- (void)safeAreaInsetsDidChange {
    [super safeAreaInsetsDidChange];
    note([@"childSafe " stringByAppendingString:insets(self.safeAreaInsets)]);
}
@end

@interface Child : UIViewController
@end
@implementation Child
- (void)viewWillTransitionToSize:(CGSize)s withTransitionCoordinator:(id<UIViewControllerTransitionCoordinator>)c {
    [super viewWillTransitionToSize:s withTransitionCoordinator:c];
    note([@"childSize " stringByAppendingString:size(s)]);
}
@end

@interface ViewController : UIViewController
@property(nonatomic, strong) UITextField *log, *state;
@property(nonatomic) BOOL locked;
@end

@implementation ViewController
- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;
    SafeView *v = [[SafeView alloc] initWithFrame:self.view.bounds];
    v.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    [self.view addSubview:v];
    Child *child = [Child new];
    [self addChildViewController:child];
    [self.view addSubview:child.view];
    [child didMoveToParentViewController:self];
    self.log = [[UITextField alloc] initWithFrame:CGRectMake(0, 100, 300, 30)];
    self.log.tag = 1;
    [self.view addSubview:self.log];
    self.state = [[UITextField alloc] initWithFrame:CGRectMake(0, 140, 300, 30)];
    self.state.tag = 2;
    [self.view addSubview:self.state];
    // Under the tests' tap once the phone is sideways (844x390).
    UIButton *lock = [UIButton buttonWithType:UIButtonTypeSystem];
    lock.frame = CGRectMake(350, 250, 100, 100);
    [lock setTitle:@"Lock" forState:UIControlStateNormal];
    [lock addTarget:self action:@selector(lock) forControlEvents:UIControlEventTouchUpInside];
    [self.view addSubview:lock];

    NSNotificationCenter *nc = NSNotificationCenter.defaultCenter;
    [UIDevice.currentDevice beginGeneratingDeviceOrientationNotifications];
    [nc addObserverForName:UIDeviceOrientationDidChangeNotification object:nil queue:nil usingBlock:^(NSNotification *n) {
        note([NSString stringWithFormat:@"device %ld animated %@", (long)UIDevice.currentDevice.orientation, n.userInfo[@"UIDeviceOrientationRotateAnimatedUserInfoKey"]]);
    }];
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    [nc addObserverForName:UIApplicationWillChangeStatusBarOrientationNotification object:nil queue:nil usingBlock:^(NSNotification *n) {
        note([NSString stringWithFormat:@"willStatusBar %@", n.userInfo[UIApplicationStatusBarOrientationUserInfoKey]]);
    }];
    [nc addObserverForName:UIApplicationDidChangeStatusBarOrientationNotification object:nil queue:nil usingBlock:^(NSNotification *n) {
        note([NSString stringWithFormat:@"didStatusBar %@", n.userInfo[UIApplicationStatusBarOrientationUserInfoKey]]);
    }];
    [nc addObserverForName:UIApplicationDidChangeStatusBarFrameNotification object:nil queue:nil usingBlock:^(NSNotification *n) {
        note([NSString stringWithFormat:@"statusBarFrame %@", size([n.userInfo[UIApplicationStatusBarFrameUserInfoKey] CGRectValue].size)]);
    }];
#pragma clang diagnostic pop
}

- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    logging = YES;
    [self show];
}

- (void)lock {
    self.locked = YES;
    [self setNeedsUpdateOfSupportedInterfaceOrientations];
}

- (UIInterfaceOrientationMask)supportedInterfaceOrientations {
    return self.locked ? UIInterfaceOrientationMaskPortrait : UIInterfaceOrientationMaskAllButUpsideDown;
}

- (void)willTransitionToTraitCollection:(UITraitCollection *)t withTransitionCoordinator:(id<UIViewControllerTransitionCoordinator>)c {
    [super willTransitionToTraitCollection:t withTransitionCoordinator:c];
    note([NSString stringWithFormat:@"willTraits v%ld", (long)t.verticalSizeClass]);
}

- (void)viewWillTransitionToSize:(CGSize)s withTransitionCoordinator:(id<UIViewControllerTransitionCoordinator>)c {
    note([NSString stringWithFormat:@"willSize %@ from %@ in %g", size(s), size(self.view.bounds.size), c.transitionDuration]);
    [super viewWillTransitionToSize:s withTransitionCoordinator:c];
    [c animateAlongsideTransition:^(id<UIViewControllerTransitionCoordinatorContext> ctx) {
        note([@"alongside " stringByAppendingString:size(self.view.bounds.size)]);
    } completion:^(id<UIViewControllerTransitionCoordinatorContext> ctx) {
        note(@"done");
        [self show];
    }];
}

- (void)viewLayoutMarginsDidChange {
    [super viewLayoutMarginsDidChange];
    note([@"margins " stringByAppendingString:insets(self.view.layoutMargins)]);
}

- (void)viewSafeAreaInsetsDidChange {
    [super viewSafeAreaInsetsDidChange];
    note([@"safe " stringByAppendingString:insets(self.view.safeAreaInsets)]);
}

- (void)traitCollectionDidChange:(UITraitCollection *)previous {
    [super traitCollectionDidChange:previous];
    note([NSString stringWithFormat:@"traits v%ld -> v%ld", (long)previous.verticalSizeClass, (long)self.traitCollection.verticalSizeClass]);
}

- (void)viewWillLayoutSubviews {
    [super viewWillLayoutSubviews];
    note([@"layout " stringByAppendingString:size(self.view.bounds.size)]);
}

- (void)show {
    UIWindowScene *scene = self.view.window.windowScene;
    self.log.text = [events componentsJoinedByString:@"; "];
    self.state.text = [NSString stringWithFormat:@"screen %@ window %@ scene %ld device %ld statusBar %@ hidden %d classes h%ld v%ld",
        size(UIScreen.mainScreen.bounds.size), size(self.view.window.bounds.size), (long)scene.interfaceOrientation,
        (long)UIDevice.currentDevice.orientation, size(scene.statusBarManager.statusBarFrame.size), scene.statusBarManager.isStatusBarHidden,
        (long)self.traitCollection.horizontalSizeClass, (long)self.traitCollection.verticalSizeClass];
    [events removeAllObjects];
}
@end

@interface SceneDelegate : UIResponder <UIWindowSceneDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation SceneDelegate
- (void)scene:(UIScene *)scene willConnectToSession:(UISceneSession *)session options:(UISceneConnectionOptions *)options {
    self.window = [[UIWindow alloc] initWithWindowScene:(UIWindowScene *)scene];
    self.window.rootViewController = [ViewController new];
    [self.window makeKeyAndVisible];
}
- (void)windowScene:(UIWindowScene *)scene didUpdateCoordinateSpace:(id<UICoordinateSpace>)previous interfaceOrientation:(UIInterfaceOrientation)o traitCollection:(UITraitCollection *)t {
    note([NSString stringWithFormat:@"scene from %@ %ld now %ld", size(previous.bounds.size), (long)o, (long)scene.interfaceOrientation]);
}
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@end
@implementation AppDelegate
@end

int main(int argc, char *argv[]) {
    events = [NSMutableArray new];
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
