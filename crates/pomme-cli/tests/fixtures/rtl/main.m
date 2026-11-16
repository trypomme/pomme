// Right-to-left layout, for tests/rtl.rs: where the layout direction comes
// from, and how Auto Layout, stack views and labels lay out in it. Each view's
// frame (in the root view) is a line `METRIC name x y width height`; each fact
// a line `VALUE name text`. scripts/swiftui/metrics-simulator.sh records it
// on the Simulator (VARIANT=<language>, with ARGS), as fixtures/rtl/ios26-<variant>.txt;
// fixtures/rtlplain is the same app with no Arabic localization.
#import <UIKit/UIKit.h>

static void value(NSString *name, id v) {
    printf("VALUE %s %s\n", name.UTF8String, [[NSString stringWithFormat:@"%@", v] stringByReplacingOccurrencesOfString:@"\n" withString:@"\\n"].UTF8String);
    fflush(stdout);
}

static NSMutableDictionary<NSString *, UIView *> *views;

static UIView *box(NSString *name, UIColor *color) {
    UIView *v = [UIView new];
    v.backgroundColor = color;
    v.translatesAutoresizingMaskIntoConstraints = NO;
    views[name] = v;
    return v;
}

static void size(UIView *v, CGFloat w, CGFloat h) {
    [v.widthAnchor constraintEqualToConstant:w].active = YES;
    [v.heightAnchor constraintEqualToConstant:h].active = YES;
}

static UIStackView *stack(NSString *name, UIStackViewDistribution d, UIStackViewAlignment a, UILayoutConstraintAxis axis, NSArray<UIView *> *items) {
    UIStackView *s = [[UIStackView alloc] initWithArrangedSubviews:items];
    s.axis = axis;
    s.distribution = d;
    s.alignment = a;
    s.spacing = 8;
    s.translatesAutoresizingMaskIntoConstraints = NO;
    views[name] = s;
    return s;
}

@interface Root : UIViewController
@end

@implementation Root
- (void)viewDidLoad {
    [super viewDidLoad];
    views = [NSMutableDictionary dictionary];
    UIView *root = self.view;
    root.backgroundColor = UIColor.whiteColor;

    // A container, and boxes pinned to its leading and trailing edges, and to its left.
    UIView *c = box(@"container", UIColor.systemGray6Color);
    [root addSubview:c];
    [NSLayoutConstraint activateConstraints:@[
        [c.leadingAnchor constraintEqualToAnchor:root.leadingAnchor constant:20],
        [c.topAnchor constraintEqualToAnchor:root.topAnchor constant:80],
        [c.widthAnchor constraintEqualToConstant:300],
        [c.heightAnchor constraintEqualToConstant:100],
    ]];
    UIView *lead = box(@"leading", UIColor.systemRedColor), *trail = box(@"trailing", UIColor.systemBlueColor), *left = box(@"left", UIColor.systemGreenColor), *center = box(@"centerOffset", UIColor.systemOrangeColor);
    for (UIView *v in @[ lead, trail, left, center ]) [c addSubview:v];
    size(lead, 50, 20); size(trail, 60, 20); size(left, 40, 20); size(center, 30, 20);
    [NSLayoutConstraint activateConstraints:@[
        [lead.leadingAnchor constraintEqualToAnchor:c.leadingAnchor constant:10],
        [lead.topAnchor constraintEqualToAnchor:c.topAnchor constant:5],
        [trail.trailingAnchor constraintEqualToAnchor:c.trailingAnchor constant:-20],
        [trail.topAnchor constraintEqualToAnchor:c.topAnchor constant:30],
        [left.leftAnchor constraintEqualToAnchor:c.leftAnchor constant:10],
        [left.topAnchor constraintEqualToAnchor:c.topAnchor constant:55],
        [center.centerXAnchor constraintEqualToAnchor:c.centerXAnchor constant:40],
        [center.topAnchor constraintEqualToAnchor:c.topAnchor constant:80],
    ]];
    // One after another: leading of the next = trailing of the first + 8.
    UIView *next = box(@"afterLeading", UIColor.systemPurpleColor);
    [c addSubview:next];
    size(next, 20, 20);
    [NSLayoutConstraint activateConstraints:@[
        [next.leadingAnchor constraintEqualToAnchor:lead.trailingAnchor constant:8],
        [next.topAnchor constraintEqualToAnchor:c.topAnchor constant:5],
    ]];

    // The same inside a view forced left-to-right.
    UIView *ltr = box(@"forcedLTR", UIColor.systemGray5Color);
    ltr.semanticContentAttribute = UISemanticContentAttributeForceLeftToRight;
    [root addSubview:ltr];
    UIView *inner = box(@"forcedLTR.leading", UIColor.systemRedColor);
    [ltr addSubview:inner];
    size(inner, 50, 20);
    [NSLayoutConstraint activateConstraints:@[
        [ltr.leadingAnchor constraintEqualToAnchor:root.leadingAnchor constant:20],
        [ltr.topAnchor constraintEqualToAnchor:root.topAnchor constant:190],
        [ltr.widthAnchor constraintEqualToConstant:300],
        [ltr.heightAnchor constraintEqualToConstant:30],
        [inner.leadingAnchor constraintEqualToAnchor:ltr.leadingAnchor constant:10],
        [inner.topAnchor constraintEqualToAnchor:ltr.topAnchor constant:5],
    ]];

    // Stack views: horizontal, three ways of sharing the width; vertical, aligned to the leading edge.
    NSArray *dists = @[ @(UIStackViewDistributionFill), @(UIStackViewDistributionFillEqually), @(UIStackViewDistributionEqualSpacing) ];
    NSArray *names = @[ @"fill", @"fillEqually", @"equalSpacing" ];
    CGFloat y = 230;
    for (NSUInteger i = 0; i < dists.count; i++) {
        NSString *n = names[i];
        UIView *a = box([n stringByAppendingString:@".a"], UIColor.systemRedColor);
        UIView *b = box([n stringByAppendingString:@".b"], UIColor.systemGreenColor);
        UIView *d = box([n stringByAppendingString:@".c"], UIColor.systemBlueColor);
        // Fill: the middle one has no width, and takes what's left. Fill
        // equally: no widths. Equal spacing: all three have one.
        if (i != 1) {
            [a.widthAnchor constraintEqualToConstant:40].active = YES;
            [d.widthAnchor constraintEqualToConstant:30].active = YES;
        }
        if (i == 2) [b.widthAnchor constraintEqualToConstant:60].active = YES;
        for (UIView *v in @[ a, b, d ]) [v.heightAnchor constraintEqualToConstant:20].active = YES;
        UIStackView *s = stack([@"stack." stringByAppendingString:n], [dists[i] integerValue], UIStackViewAlignmentCenter, UILayoutConstraintAxisHorizontal, @[ a, b, d ]);
        [root addSubview:s];
        [NSLayoutConstraint activateConstraints:@[
            [s.leadingAnchor constraintEqualToAnchor:root.leadingAnchor constant:20],
            [s.topAnchor constraintEqualToAnchor:root.topAnchor constant:y],
            [s.widthAnchor constraintEqualToConstant:300],
            [s.heightAnchor constraintEqualToConstant:30],
        ]];
        y += 40;
    }
    UIView *va = box(@"vertical.a", UIColor.systemRedColor), *vb = box(@"vertical.b", UIColor.systemGreenColor);
    size(va, 80, 20); size(vb, 140, 20);
    UIStackView *vs = stack(@"stack.vertical", UIStackViewDistributionFill, UIStackViewAlignmentLeading, UILayoutConstraintAxisVertical, @[ va, vb ]);
    [root addSubview:vs];
    [NSLayoutConstraint activateConstraints:@[
        [vs.leadingAnchor constraintEqualToAnchor:root.leadingAnchor constant:20],
        [vs.topAnchor constraintEqualToAnchor:root.topAnchor constant:y],
        [vs.widthAnchor constraintEqualToConstant:300],
    ]];

    // A label with natural alignment (its text, the app's translation).
    UILabel *label = [UILabel new];
    label.text = NSLocalizedString(@"title", nil);
    label.backgroundColor = UIColor.systemYellowColor;
    label.frame = CGRectMake(20, 450, 300, 30);
    [root addSubview:label];
    views[@"label"] = label;

    // Text in Arabic and Hebrew, and mixed with English and numbers.
    NSArray *texts = @[ @"مرحبا بالعالم", @"שלום עולם", @"مرحبا Pomme 123", @"Pomme مرحبا 123" ];
    NSArray *textNames = @[ @"text.arabic", @"text.hebrew", @"text.mixedArabicFirst", @"text.mixedLatinFirst" ];
    for (NSUInteger i = 0; i < texts.count; i++) {
        UILabel *l = [UILabel new];
        l.text = texts[i];
        l.backgroundColor = UIColor.systemTealColor;
        l.frame = CGRectMake(20, 500 + 40 * i, 300, 30);
        [root addSubview:l];
        views[textNames[i]] = l;
    }
}

- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    UIView *root = self.view;
    value(@"app.userInterfaceLayoutDirection", @(UIApplication.sharedApplication.userInterfaceLayoutDirection));
    value(@"root.effectiveUserInterfaceLayoutDirection", @(root.effectiveUserInterfaceLayoutDirection));
    value(@"root.semanticContentAttribute", @(root.semanticContentAttribute));
    value(@"root.traitCollection.layoutDirection", @(root.traitCollection.layoutDirection));
    value(@"forcedLTR.effective", @(views[@"forcedLTR"].effectiveUserInterfaceLayoutDirection));
    value(@"forcedLTR.leading.effective", @(views[@"forcedLTR.leading"].effectiveUserInterfaceLayoutDirection));
    value(@"forcedLTR.leading.traitCollection.layoutDirection", @(views[@"forcedLTR.leading"].traitCollection.layoutDirection));
    for (NSInteger a = 0; a <= 4; a++) {
        value([NSString stringWithFormat:@"forSemantic.%ld", (long)a], @([UIView userInterfaceLayoutDirectionForSemanticContentAttribute:a]));
    }
    value(@"locale.characterDirection.ar", @([NSLocale characterDirectionForLanguage:@"ar"]));
    value(@"bundle.preferredLocalizations", NSBundle.mainBundle.preferredLocalizations.firstObject);
    value(@"label.text", ((UILabel *)views[@"label"]).text);
    value(@"label.textAlignment", @(((UILabel *)views[@"label"]).textAlignment));
    value(@"paragraph.defaultWritingDirection", @([NSParagraphStyle defaultWritingDirectionForLanguage:nil]));
    for (NSString *name in [views.allKeys sortedArrayUsingSelector:@selector(compare:)]) {
        CGRect r = [views[name] convertRect:views[name].bounds toView:root];
        printf("METRIC %s %.2f %.2f %.2f %.2f\n", name.UTF8String, r.origin.x, r.origin.y, r.size.width, r.size.height);
    }
    fflush(stdout);
}
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)options {
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    self.window.rootViewController = [Root new];
    [self.window makeKeyAndVisible];
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
