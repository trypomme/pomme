// Bar button items in the navigation bar, for tests/bar_items.rs.
// PROBE_VARIANT picks the case: `add` (one system item, trailing), `two`
// (add and edit), `both` (a leading title item and a trailing done item),
// `title` (a trailing title item), `image` (a symbol), `custom` (a 30-pt
// custom view), `prominent` (a prominent done item), `separate` (two items,
// the second with hidesSharedBackground), `pushed` (a trailing item on a
// pushed screen, beside the back button). After a second the bar's views are
// listed (`VIEW class x y w h text [bg=] [radius=]`, in the window), and each
// item's tap target (`VALUE`). scripts/swiftui/metrics-simulator.sh records it.
#import <UIKit/UIKit.h>

static NSString *variant(void) {
    const char *v = getenv("PROBE_VARIANT");
    return v && *v ? @(v) : @"add";
}

static void dump(UIView *v, UIView *window) {
    CGRect r = [v convertRect:v.bounds toView:window];
    NSString *text = [v isKindOfClass:UILabel.class] ? ((UILabel *)v).text : @"";
    if (!v.hidden && v.alpha > 0.01 && r.size.width > 0) {
        NSString *extra = @"";
        if (v.backgroundColor) {
            CGFloat c[4] = {0};
            [[v.backgroundColor resolvedColorWithTraitCollection:v.traitCollection] getRed:&c[0] green:&c[1] blue:&c[2] alpha:&c[3]];
            extra = [extra stringByAppendingFormat:@" bg=%d,%d,%d,%d", (int)lround(c[0] * 255), (int)lround(c[1] * 255), (int)lround(c[2] * 255), (int)lround(c[3] * 255)];
        }
        if (v.layer.cornerRadius > 0) extra = [extra stringByAppendingFormat:@" radius=%.2f", v.layer.cornerRadius];
        if ([v isKindOfClass:UILabel.class]) {
            UILabel *l = (UILabel *)v;
            CGFloat c[4] = {0};
            [[l.textColor resolvedColorWithTraitCollection:v.traitCollection] getRed:&c[0] green:&c[1] blue:&c[2] alpha:&c[3]];
            extra = [extra stringByAppendingFormat:@" font=%.1f/%.2f color=%d,%d,%d,%d", l.font.pointSize, [[l.font.fontDescriptor objectForKey:UIFontDescriptorTraitsAttribute][UIFontWeightTrait] doubleValue], (int)lround(c[0] * 255), (int)lround(c[1] * 255), (int)lround(c[2] * 255), (int)lround(c[3] * 255)];
        }
        printf("VIEW %s %.2f %.2f %.2f %.2f %s%s\n", NSStringFromClass(v.class).UTF8String, r.origin.x, r.origin.y, r.size.width, r.size.height, (text ?: @"").UTF8String, extra.UTF8String);
    }
    for (UIView *s in v.subviews) dump(s, window);
}

@interface Screen : UIViewController
@end

@implementation Screen
- (void)tapped:(UIBarButtonItem *)item {
    printf("VALUE tapped %s\n", (item.title ?: @"(no title)").UTF8String);
    fflush(stdout);
}
- (UIBarButtonItem *)system:(UIBarButtonSystemItem)s {
    return [[UIBarButtonItem alloc] initWithBarButtonSystemItem:s target:self action:@selector(tapped:)];
}
- (UIBarButtonItem *)titled:(NSString *)t {
    return [[UIBarButtonItem alloc] initWithTitle:t style:UIBarButtonItemStylePlain target:self action:@selector(tapped:)];
}
- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;
    NSString *v = variant();
    UINavigationItem *n = self.navigationItem;
    if ([v isEqual:@"add"] || [v isEqual:@"pushed"]) {
        n.rightBarButtonItem = [self system:UIBarButtonSystemItemAdd];
    } else if ([v isEqual:@"two"]) {
        n.rightBarButtonItems = @[ [self system:UIBarButtonSystemItemAdd], [self system:UIBarButtonSystemItemEdit] ];
    } else if ([v isEqual:@"both"]) {
        n.leftBarButtonItem = [self titled:@"Cancel"];
        n.rightBarButtonItem = [self system:UIBarButtonSystemItemDone];
    } else if ([v isEqual:@"title"]) {
        n.rightBarButtonItem = [self titled:@"Save"];
    } else if ([v isEqual:@"image"]) {
        n.rightBarButtonItem = [[UIBarButtonItem alloc] initWithImage:[UIImage systemImageNamed:@"gear"] style:UIBarButtonItemStylePlain target:self action:@selector(tapped:)];
    } else if ([v isEqual:@"custom"]) {
        UIView *c = [[UIView alloc] initWithFrame:CGRectMake(0, 0, 30, 30)];
        c.backgroundColor = UIColor.redColor;
        n.rightBarButtonItem = [[UIBarButtonItem alloc] initWithCustomView:c];
    } else if ([v isEqual:@"prominent"]) {
        UIBarButtonItem *done = [self system:UIBarButtonSystemItemDone];
        done.style = UIBarButtonItemStyleProminent;
        n.rightBarButtonItem = done;
    } else if ([v isEqual:@"separate"]) {
        UIBarButtonItem *b = [self system:UIBarButtonSystemItemEdit];
        b.hidesSharedBackground = YES;
        n.rightBarButtonItems = @[ [self system:UIBarButtonSystemItemAdd], b ];
    }
}
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)options {
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    BOOL pushed = [variant() isEqual:@"pushed"];
    UIViewController *root = pushed ? [UIViewController new] : [Screen new];
    root.view.backgroundColor = UIColor.whiteColor;
    root.title = @"Items";
    UINavigationController *nav = [[UINavigationController alloc] initWithRootViewController:root];
    if (pushed) {
        Screen *s = [Screen new];
        s.title = @"Pushed";
        [nav pushViewController:s animated:NO];
    }
    self.window.rootViewController = nav;
    [self.window makeKeyAndVisible];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC), dispatch_get_main_queue(), ^{
        dump(nav.navigationBar, self.window);
        fflush(stdout);
    });
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
