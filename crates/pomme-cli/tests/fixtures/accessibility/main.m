// A small UIKit app for tests/accessibility.rs: what a host's screen reader
// sees (UIKit's default elements, a container that names a group, an element
// with its own activation, a hidden subtree) and what it can do (activate,
// adjust), and a modal view that hides what's beside it.
#import <UIKit/UIKit.h>

// An element that handles VoiceOver's double-tap itself, as Flutter's semantics nodes do.
@interface Activatable : UIView
@property(nonatomic) NSInteger count;
@end

@implementation Activatable
- (BOOL)isAccessibilityElement {
    return YES;
}
- (NSString *)accessibilityLabel {
    return [NSString stringWithFormat:@"Activated %ld times", (long)self.count];
}
- (UIAccessibilityTraits)accessibilityTraits {
    return UIAccessibilityTraitButton;
}
- (BOOL)accessibilityActivate {
    self.count++;
    return YES;
}
@end

@interface ViewController : UIViewController
@property(nonatomic, strong) UILabel *count;
@property(nonatomic) NSInteger taps;
@end

@implementation ViewController
- (UILabel *)label:(NSString *)text frame:(CGRect)frame in:(UIView *)parent {
    UILabel *l = [[UILabel alloc] initWithFrame:frame];
    l.text = text;
    [parent addSubview:l];
    return l;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;
    UILabel *title = [self label:@"Accessibility" frame:CGRectMake(20, 60, 350, 40) in:self.view];
    title.accessibilityTraits = UIAccessibilityTraitHeader;

    UIButton *tap = [UIButton buttonWithType:UIButtonTypeSystem];
    tap.frame = CGRectMake(20, 110, 160, 44);
    [tap setTitle:@"Tap me" forState:UIControlStateNormal];
    tap.accessibilityIdentifier = @"tap";
    tap.accessibilityHint = @"Counts taps";
    [tap addTarget:self action:@selector(tapped) forControlEvents:UIControlEventTouchUpInside];
    [self.view addSubview:tap];
    self.count = [self label:@"Tapped 0 times" frame:CGRectMake(200, 110, 170, 44) in:self.view];

    // A container that names its elements: VoiceOver says "Settings" on the way in.
    UIView *settings = [[UIView alloc] initWithFrame:CGRectMake(0, 170, 390, 120)];
    settings.accessibilityLabel = @"Settings";
    [self.view addSubview:settings];
    UISwitch *wifi = [[UISwitch alloc] initWithFrame:CGRectMake(20, 10, 0, 0)];
    wifi.on = YES;
    wifi.accessibilityLabel = @"Wi-Fi";
    [settings addSubview:wifi];
    UISlider *volume = [[UISlider alloc] initWithFrame:CGRectMake(20, 60, 350, 40)];
    volume.value = 0.5;
    volume.accessibilityLabel = @"Volume";
    [settings addSubview:volume];

    UITextField *name = [[UITextField alloc] initWithFrame:CGRectMake(20, 300, 350, 40)];
    name.placeholder = @"Your name";
    [self.view addSubview:name];

    Activatable *custom = [[Activatable alloc] initWithFrame:CGRectMake(20, 350, 350, 44)];
    custom.accessibilityIdentifier = @"custom";
    [self.view addSubview:custom];

    // Hidden from accessibility, though on screen.
    UIView *decoration = [[UIView alloc] initWithFrame:CGRectMake(20, 400, 350, 44)];
    decoration.accessibilityElementsHidden = YES;
    [self label:@"Decoration" frame:CGRectMake(0, 0, 350, 44) in:decoration];
    [self.view addSubview:decoration];

    UIButton *alert = [UIButton buttonWithType:UIButtonTypeSystem];
    alert.frame = CGRectMake(20, 450, 160, 44);
    [alert setTitle:@"Show alert" forState:UIControlStateNormal];
    [alert addTarget:self action:@selector(showAlert) forControlEvents:UIControlEventTouchUpInside];
    [self.view addSubview:alert];
}

- (void)tapped {
    self.taps++;
    self.count.text = [NSString stringWithFormat:@"Tapped %ld times", (long)self.taps];
}

// A view that's modal for VoiceOver: only it is read while it's up.
- (void)showAlert {
    UIView *box = [[UIView alloc] initWithFrame:CGRectMake(45, 300, 300, 160)];
    box.backgroundColor = UIColor.lightGrayColor;
    box.accessibilityViewIsModal = YES;
    [self label:@"Are you sure?" frame:CGRectMake(20, 20, 260, 40) in:box];
    UIButton *ok = [UIButton buttonWithType:UIButtonTypeSystem];
    ok.frame = CGRectMake(20, 90, 260, 44);
    [ok setTitle:@"OK" forState:UIControlStateNormal];
    [box addSubview:ok];
    [self.view addSubview:box];
}
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)application didFinishLaunchingWithOptions:(NSDictionary *)options {
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    self.window.rootViewController = [ViewController new];
    [self.window makeKeyAndVisible];
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass([AppDelegate class]));
    }
}
