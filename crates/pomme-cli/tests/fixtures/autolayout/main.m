// A small UIKit app laid out with Auto Layout, for tests/autolayout.rs.
// Every view is a subclass named for its role, so the view dump identifies it;
// tags count the safe-area callbacks.
#import <UIKit/UIKit.h>

@interface Header : UIView
@end
@implementation Header
@end
@interface Footer : UIView
@end
@implementation Footer
@end
@interface Box : UIView
@end
@implementation Box
@end
@interface Margined : UIView
@end
@implementation Margined
@end
@interface Spaced : UIView
@end
@implementation Spaced
@end
@interface Badge : UIView
@end
@implementation Badge
- (CGSize)intrinsicContentSize {
    return CGSizeMake(60, 24);
}
@end
@interface Above : UIView
@end
@implementation Above
@end

// Counts its safe-area changes in its tag.
@interface Listener : UIView
@end
@implementation Listener
- (void)safeAreaInsetsDidChange {
    [super safeAreaInsetsDidChange];
    self.tag += 1;
}
@end

@interface ViewController : UIViewController
@property(nonatomic, strong) UIView *counter;
@end

@implementation ViewController
- (void)loadView {
    self.view = [[Listener alloc] initWithFrame:UIScreen.mainScreen.bounds];
}

- (void)viewDidLoad {
    [super viewDidLoad];
    UIView *root = self.view;
    UILayoutGuide *safe = root.safeAreaLayoutGuide;

    // Pinned to the safe area's top, full width, 50 pt tall.
    UIView *header = [Header new];
    // At the safe area's bottom, inset 16 pt at the sides, 44 pt tall.
    UIView *footer = [Footer new];
    // Centered, 100 pt wide, half as tall as wide.
    UIView *box = [Box new];
    // At the margins, 8 pt below the header, 30 pt tall (the item API).
    UIView *margined = [Margined new];
    // Inside a custom guide that spans the header's bottom to the footer's top.
    UIView *spaced = [Spaced new];
    // Its size from intrinsicContentSize, below the box.
    UIView *badge = [Badge new];
    // Sits on the keyboard (or the safe area's bottom while it's down).
    UIView *above = [Above new];
    // Counts the view controller's callbacks in its tag.
    self.counter = [UIView new];
    for (UIView *v in @[ header, footer, box, margined, spaced, badge, above ]) {
        v.translatesAutoresizingMaskIntoConstraints = NO;
        [root addSubview:v];
    }
    [root addSubview:self.counter];

    UILayoutGuide *between = [UILayoutGuide new];
    [root addLayoutGuide:between];

    [NSLayoutConstraint activateConstraints:@[
        [header.topAnchor constraintEqualToAnchor:safe.topAnchor],
        [header.leadingAnchor constraintEqualToAnchor:root.leadingAnchor],
        [header.trailingAnchor constraintEqualToAnchor:root.trailingAnchor],
        [header.heightAnchor constraintEqualToConstant:50],

        [footer.bottomAnchor constraintEqualToAnchor:safe.bottomAnchor],
        [footer.leadingAnchor constraintEqualToAnchor:safe.leadingAnchor constant:16],
        [footer.trailingAnchor constraintEqualToAnchor:safe.trailingAnchor constant:-16],
        [footer.heightAnchor constraintEqualToConstant:44],

        [box.centerXAnchor constraintEqualToAnchor:root.centerXAnchor],
        [box.centerYAnchor constraintEqualToAnchor:root.centerYAnchor],
        [box.widthAnchor constraintEqualToConstant:100],
        [box.heightAnchor constraintEqualToAnchor:box.widthAnchor multiplier:0.5],

        [between.topAnchor constraintEqualToAnchor:header.bottomAnchor],
        [between.bottomAnchor constraintEqualToAnchor:footer.topAnchor],
        [between.leadingAnchor constraintEqualToAnchor:root.leadingAnchor],
        [between.widthAnchor constraintEqualToConstant:10],
        [spaced.centerYAnchor constraintEqualToAnchor:between.centerYAnchor],
        [spaced.leadingAnchor constraintEqualToAnchor:between.trailingAnchor],
        [spaced.widthAnchor constraintEqualToConstant:20],
        [spaced.heightAnchor constraintEqualToConstant:20],

        [badge.topAnchor constraintEqualToSystemSpacingBelowAnchor:box.bottomAnchor multiplier:1],
        [badge.centerXAnchor constraintEqualToAnchor:box.centerXAnchor],

        [above.bottomAnchor constraintEqualToAnchor:root.keyboardLayoutGuide.topAnchor],
        [above.trailingAnchor constraintEqualToAnchor:root.trailingAnchor],
        [above.widthAnchor constraintEqualToConstant:40],
        [above.heightAnchor constraintEqualToConstant:10],
    ]];
    [root addConstraints:@[
        [NSLayoutConstraint constraintWithItem:margined attribute:NSLayoutAttributeLeadingMargin relatedBy:NSLayoutRelationEqual toItem:root attribute:NSLayoutAttributeLeadingMargin multiplier:1 constant:0],
        [NSLayoutConstraint constraintWithItem:margined attribute:NSLayoutAttributeTrailing relatedBy:NSLayoutRelationEqual toItem:root attribute:NSLayoutAttributeTrailingMargin multiplier:1 constant:0],
        [NSLayoutConstraint constraintWithItem:margined attribute:NSLayoutAttributeTop relatedBy:NSLayoutRelationEqual toItem:header attribute:NSLayoutAttributeBottom multiplier:1 constant:8],
        [NSLayoutConstraint constraintWithItem:margined attribute:NSLayoutAttributeHeight relatedBy:NSLayoutRelationEqual toItem:nil attribute:NSLayoutAttributeNotAnAttribute multiplier:1 constant:30],
    ]];
}

- (void)viewSafeAreaInsetsDidChange {
    [super viewSafeAreaInsetsDidChange];
    self.counter.tag += 1;
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
