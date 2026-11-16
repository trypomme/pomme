// A shake (tests/shake.rs): what UIKit sends, to whom, in what order. The
// view controller is the first responder; the window, the app and the app
// delegate log what reaches them too. On the Simulator: Device ▸ Shake, or
// SHAKE=1 scripts/swiftui/metrics-simulator.sh shake (the same notification).
#import <UIKit/UIKit.h>

static NSMutableArray<NSString *> *events;

static void note(NSString *who, NSString *what, UIEvent *e) {
    [events addObject:[NSString stringWithFormat:@"%@.%@(type %ld subtype %ld)", who, what, (long)e.type, (long)e.subtype]];
    printf("VALUE events %s\n", [events componentsJoinedByString:@","].UTF8String);
    fflush(stdout);
}

#define LOG_MOTION(who) \
    - (void)motionBegan:(UIEventSubtype)m withEvent:(UIEvent *)e { note(who, @"motionBegan", e); [super motionBegan:m withEvent:e]; } \
    - (void)motionEnded:(UIEventSubtype)m withEvent:(UIEvent *)e { note(who, @"motionEnded", e); [super motionEnded:m withEvent:e]; } \
    - (void)motionCancelled:(UIEventSubtype)m withEvent:(UIEvent *)e { note(who, @"motionCancelled", e); [super motionCancelled:m withEvent:e]; }

@interface ViewController : UIViewController
@end
@implementation ViewController
- (BOOL)canBecomeFirstResponder { return YES; }
- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;
}
- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    printf("VALUE becameFirstResponder %d\n", [self becomeFirstResponder]);
    fflush(stdout);
}
LOG_MOTION(@"controller")
@end

@interface Window : UIWindow
@end
@implementation Window
LOG_MOTION(@"window")
@end

@interface App : UIApplication
@end
@implementation App
LOG_MOTION(@"application")
@end

@interface SceneDelegate : UIResponder <UIWindowSceneDelegate>
@property(nonatomic, strong) UIWindow *window;
@end
@implementation SceneDelegate
- (void)scene:(UIScene *)scene willConnectToSession:(UISceneSession *)session options:(UISceneConnectionOptions *)options {
    self.window = [[Window alloc] initWithWindowScene:(UIWindowScene *)scene];
    self.window.rootViewController = [ViewController new];
    [self.window makeKeyAndVisible];
}
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@end
@implementation AppDelegate
LOG_MOTION(@"delegate")
@end

int main(int argc, char *argv[]) {
    events = [NSMutableArray new];
    @autoreleasepool {
        return UIApplicationMain(argc, argv, NSStringFromClass(App.class), NSStringFromClass(AppDelegate.class));
    }
}
