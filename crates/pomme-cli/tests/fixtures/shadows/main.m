// CALayer shadows, for tests/shadows.rs (the same scene as scripts/measure/shadows,
// which measured them in the iOS Simulator): red 80 pt squares on white.
//   row 1 (y 120): shadowRadius 4, 10, 20 (opacity 1, no offset)
//   row 2 (y 320): radius 10 offset (0, 10); radius 10 with corner radius 20;
//                  radius 10 at opacity 0.5
//   row 3 (y 520): radius 10 on a view with no background whose subview (a
//                  40 pt square in its middle) is all that's drawn
#import <UIKit/UIKit.h>

static UIView *square(UIView *parent, CGFloat x, CGFloat y, CGFloat radius, CGSize offset, CGFloat corner, float opacity) {
    UIView *v = [[UIView alloc] initWithFrame:CGRectMake(x, y, 80, 80)];
    v.backgroundColor = UIColor.redColor;
    v.layer.cornerRadius = corner;
    v.layer.shadowColor = UIColor.blackColor.CGColor;
    v.layer.shadowOpacity = opacity;
    v.layer.shadowRadius = radius;
    v.layer.shadowOffset = offset;
    [parent addSubview:v];
    return v;
}

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)options {
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    UIViewController *vc = [UIViewController new];
    vc.view.backgroundColor = UIColor.whiteColor;
    self.window.rootViewController = vc;
    [self.window makeKeyAndVisible];
    UIView *root = vc.view;
    square(root, 20, 120, 4, CGSizeZero, 0, 1);
    square(root, 150, 120, 10, CGSizeZero, 0, 1);
    square(root, 280, 120, 20, CGSizeZero, 0, 1);
    square(root, 20, 320, 10, CGSizeMake(0, 10), 0, 1);
    square(root, 150, 320, 10, CGSizeZero, 20, 1);
    square(root, 280, 320, 10, CGSizeZero, 0, 0.5);
    UIView *holder = square(root, 150, 520, 10, CGSizeZero, 0, 1);
    holder.backgroundColor = nil;
    UIView *inner = [[UIView alloc] initWithFrame:CGRectMake(20, 20, 40, 40)];
    inner.backgroundColor = UIColor.redColor;
    [holder addSubview:inner];
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
