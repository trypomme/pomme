// Alerts and action sheets (UIAlertController), as UIKit shows them on iOS 26
// (openspec/changes/add-alert-controller). PROBE_VARIANT picks the case:
//   alert2   title, message, cancel + default (titleonly, messageonly, long: the same with
//            only a title, only a message, a message that wraps)
//   alert3   three actions (the cancel added first)
//   styles   destructive + preferred default
//   disabled a disabled action and an enabled one
//   sheet    action sheet: title, message, cancel (added first), two defaults, a destructive
// The alert is presented at 1 s. Half a second after it is on screen the probe
// prints the frames (in window points) of what's drawn: DUMP lines for every
// view, VALUE lines for the parts tests compare. Events are logged as EVENT
// lines with the time since launch.
#import <UIKit/UIKit.h>

static CFAbsoluteTime t0;
static void event(NSString *what) {
    printf("EVENT %.3f %s\n", CFAbsoluteTimeGetCurrent() - t0, what.UTF8String);
    fflush(stdout);
}

static NSString *rect(CGRect r) {
    return [NSString stringWithFormat:@"%.2f %.2f %.2f %.2f", r.origin.x, r.origin.y, r.size.width, r.size.height];
}

static NSString *rgba(UIColor *c) {
    if (!c) return @"nil";
    CGFloat r = 0, g = 0, b = 0, a = 0;
    [c getRed:&r green:&g blue:&b alpha:&a];
    return [NSString stringWithFormat:@"%.3f %.3f %.3f %.3f", r, g, b, a];
}

/// The colour the label's text is drawn in (its attributed text's, else textColor).
static UIColor *drawnColor(UILabel *l) {
    UIColor *c = l.attributedText.length ? [l.attributedText attribute:NSForegroundColorAttributeName atIndex:0 effectiveRange:NULL] : nil;
    return c ?: l.textColor;
}

static void collect(UIView *v, NSString *cls, NSMutableArray *out) {
    if ([NSStringFromClass(v.class) isEqualToString:cls]) [out addObject:v];
    for (UIView *s in v.subviews) collect(s, cls, out);
}

static void dump(UIView *v, UIWindow *w, int depth) {
    NSMutableString *line = [NSMutableString stringWithFormat:@"DUMP %*s%@ frame %@ alpha %.2f", depth * 2, "", NSStringFromClass(v.class), rect([v convertRect:v.bounds toView:w]), v.alpha];
    if (v.hidden) [line appendString:@" hidden"];
    if (v.backgroundColor) [line appendFormat:@" bg %@", rgba(v.backgroundColor)];
    if (v.layer.cornerRadius > 0) [line appendFormat:@" radius %.2f%@", v.layer.cornerRadius, [v.layer.cornerCurve isEqualToString:@"continuous"] ? @" continuous" : @""];
    if ([v isKindOfClass:UILabel.class]) {
        UILabel *l = (UILabel *)v;
        [line appendFormat:@" text \"%@\" font %@ %.2f color %@ lines %ld align %ld", [l.text stringByReplacingOccurrencesOfString:@"\n" withString:@"\\n"], l.font.fontName, l.font.pointSize, rgba(drawnColor(l)), (long)l.numberOfLines, (long)l.textAlignment];
    }
    printf("%s\n", line.UTF8String);
    for (UIView *s in v.subviews) dump(s, w, depth + 1);
}

/// The label showing `text` (first found, depth first).
static UILabel *labelWithText(UIView *v, NSString *text) {
    if ([v isKindOfClass:UILabel.class] && [((UILabel *)v).text isEqualToString:text]) return (UILabel *)v;
    for (UIView *s in v.subviews) {
        UILabel *l = labelWithText(s, text);
        if (l) return l;
    }
    return nil;
}

@interface ViewController : UIViewController
@property(nonatomic, strong) UIAlertController *alert;
@property(nonatomic, strong) CADisplayLink *link;
@end
@implementation ViewController
/// Every layer under `l` with an animation running: its view's class, the keys, and what's drawn.
static void animating(CALayer *l, NSString *path, double t) {
    if (l.animationKeys.count) {
        CALayer *p = l.presentationLayer ?: l;
        id d = l.delegate;
        NSString *who = d ? NSStringFromClass([d class]) : NSStringFromClass(l.class);
        printf("ANIM %.4f %s %s keys %s opacity %.3f m11 %.4f m22 %.4f pos %.2f %.2f bounds %.2f %.2f\n", t, path.UTF8String, who.UTF8String,
               [l.animationKeys componentsJoinedByString:@","].UTF8String, p.opacity, p.transform.m11, p.transform.m22, p.position.x, p.position.y, p.bounds.size.width, p.bounds.size.height);
    }
    NSUInteger i = 0;
    for (CALayer *s in l.sublayers) animating(s, [NSString stringWithFormat:@"%@.%lu", path, (unsigned long)i++], t);
}

/// Each frame while the alert comes and goes: its opacity and scale, and the dimming's opacity, as drawn.
- (void)frame:(CADisplayLink *)l {
    UIView *box = self.alert.view;
    UIView *dim = box.superview.subviews.firstObject;
    CALayer *bp = box.layer.presentationLayer ?: box.layer, *dp = dim.layer.presentationLayer ?: dim.layer;
    CGRect f = box.window ? [box convertRect:box.bounds toView:nil] : CGRectMake(0, 0, 0, 0);
    if (box.window) animating(box.window.layer, @"w", CFAbsoluteTimeGetCurrent() - t0);
    printf("FRAME %.4f box %.3f scale %.4f y %.2f dim %.3f window %d\n", CFAbsoluteTimeGetCurrent() - t0, bp.opacity, bp.transform.m11, f.origin.y + bp.transform.m42, dp ? dp.opacity : -1.0, box.window != nil);
}
- (void)viewDidLoad {
    [super viewDidLoad];
    // "<case>-white" / "-black" / "-orange": the screen behind in that colour (teal otherwise).
    NSString *full = NSProcessInfo.processInfo.environment[@"PROBE_VARIANT"] ?: @"";
    self.view.backgroundColor = [full hasSuffix:@"-white"] ? UIColor.whiteColor
                              : [full hasSuffix:@"-black"] ? UIColor.blackColor
                              : [full hasSuffix:@"-orange"] ? [UIColor colorWithRed:1 green:0.5 blue:0 alpha:1]
                                                            : UIColor.systemTealColor;
    UILabel *l = [[UILabel alloc] initWithFrame:CGRectMake(20, 200, 350, 40)];
    l.text = @"Behind the alert";
    [self.view addSubview:l];
}
- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC), dispatch_get_main_queue(), ^{ [self show]; });
}
- (void)show {
    // "<case>-<how it ends>": -tap (AXe taps from outside the app), -code (dismissed in code at 3.5 s).
    NSString *full = NSProcessInfo.processInfo.environment[@"PROBE_VARIANT"] ?: @"alert2";
    NSString *variant = [full componentsSeparatedByString:@"-"].firstObject;
    BOOL sheet = [variant isEqualToString:@"sheet"];
    // titleonly / messageonly: one of the two; long: a message that wraps.
    NSString *title = [variant isEqualToString:@"messageonly"] ? nil : @"Delete the note?";
    NSString *message = [variant isEqualToString:@"titleonly"] ? nil
                      : [variant isEqualToString:@"long"] ? @"This note and its 12 attachments will be deleted from all of your devices. This can't be undone."
                                                           : @"This can't be undone.";
    UIAlertController *a = [UIAlertController alertControllerWithTitle:title
                                                               message:message
                                                        preferredStyle:sheet ? UIAlertControllerStyleActionSheet : UIAlertControllerStyleAlert];
    void (^h)(UIAlertAction *) = ^(UIAlertAction *x) {
        event([NSString stringWithFormat:@"handler %@ presented %@", x.title, self.presentedViewController ? @"yes" : @"no"]);
    };
    NSMutableArray<NSString *> *titles = [NSMutableArray array];
    if ([@[ @"alert2", @"titleonly", @"messageonly", @"long" ] containsObject:variant]) {
        [a addAction:[UIAlertAction actionWithTitle:@"Cancel" style:UIAlertActionStyleCancel handler:h]];
        [a addAction:[UIAlertAction actionWithTitle:@"OK" style:UIAlertActionStyleDefault handler:h]];
    } else if ([variant isEqualToString:@"alert3"]) {
        // (The cancel first: where does UIKit put it?)
        [a addAction:[UIAlertAction actionWithTitle:@"Cancel" style:UIAlertActionStyleCancel handler:h]];
        [a addAction:[UIAlertAction actionWithTitle:@"Save" style:UIAlertActionStyleDefault handler:h]];
        [a addAction:[UIAlertAction actionWithTitle:@"Don't Save" style:UIAlertActionStyleDestructive handler:h]];
    } else if ([variant isEqualToString:@"styles"]) {
        [a addAction:[UIAlertAction actionWithTitle:@"Delete" style:UIAlertActionStyleDestructive handler:h]];
        UIAlertAction *keep = [UIAlertAction actionWithTitle:@"Keep" style:UIAlertActionStyleDefault handler:h];
        [a addAction:keep];
        a.preferredAction = keep;
    } else if ([variant isEqualToString:@"disabled"]) {
        UIAlertAction *off = [UIAlertAction actionWithTitle:@"Send" style:UIAlertActionStyleDefault handler:h];
        off.enabled = NO;
        [a addAction:off];
        [a addAction:[UIAlertAction actionWithTitle:@"Cancel" style:UIAlertActionStyleCancel handler:h]];
    } else {
        [a addAction:[UIAlertAction actionWithTitle:@"Cancel" style:UIAlertActionStyleCancel handler:h]];
        [a addAction:[UIAlertAction actionWithTitle:@"Share" style:UIAlertActionStyleDefault handler:h]];
        [a addAction:[UIAlertAction actionWithTitle:@"Duplicate" style:UIAlertActionStyleDefault handler:h]];
        [a addAction:[UIAlertAction actionWithTitle:@"Delete" style:UIAlertActionStyleDestructive handler:h]];
    }
    for (UIAlertAction *x in a.actions) [titles addObject:x.title];
    self.alert = a;
    self.link = [CADisplayLink displayLinkWithTarget:self selector:@selector(frame:)];
    [self.link addToRunLoop:NSRunLoop.mainRunLoop forMode:NSRunLoopCommonModes];
    if ([full hasSuffix:@"-code"])
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(2.5 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
            event(@"dismiss in code");
            [self dismissViewControllerAnimated:YES completion:^{ event([NSString stringWithFormat:@"dismissed presented %@", self.presentedViewController ? @"yes" : @"no"]); }];
        });
    event(@"present");
    [self presentViewController:a animated:YES completion:^{
        event(@"presented");
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC / 2), dispatch_get_main_queue(), ^{
            UIWindow *w = a.view.window;
            printf("VALUE window %s level %.0f\n", NSStringFromClass(w.class).UTF8String, w.windowLevel);
            printf("VALUE sameWindow %d\n", w == self.view.window);
            printf("VALUE alertView %s\n", rect([a.view convertRect:a.view.bounds toView:w]).UTF8String);
            NSMutableArray *texts = [NSMutableArray array];
            if (a.title) [texts addObject:a.title];
            if (a.message) [texts addObject:a.message];
            for (NSString *s in [texts arrayByAddingObjectsFromArray:titles]) {
                UILabel *l = labelWithText(w, s);
                NSString *key = [[s componentsSeparatedByCharactersInSet:NSCharacterSet.alphanumericCharacterSet.invertedSet] componentsJoinedByString:@""];
                if (l)
                    printf("VALUE label%s %s font %s %.2f color %s align %ld\n", key.UTF8String, rect([l convertRect:l.bounds toView:w]).UTF8String, l.font.fontName.UTF8String, l.font.pointSize, rgba(drawnColor(l)).UTF8String, (long)l.textAlignment);
                else
                    printf("VALUE label%s none\n", key.UTF8String);
            }
            NSMutableArray *found = [NSMutableArray array];
            collect(w, @"_UIAlertControllerFilledBackgroundView", found);
            for (NSUInteger i = 0; i < found.count; i++) {
                UIView *fill = [found[i] subviews].firstObject ?: found[i];
                printf("VALUE button%lu %s bg %s radius %.2f\n", (unsigned long)i, rect([fill convertRect:fill.bounds toView:w]).UTF8String, rgba(fill.backgroundColor).UTF8String, fill.layer.cornerRadius);
            }
            [found removeAllObjects];
            collect(w, @"_UIAlertControllerPhoneTVMacView", found);
            for (UIView *box in found)
                printf("VALUE box %s radius %.2f\n", rect([box convertRect:box.bounds toView:w]).UTF8String, box.layer.cornerRadius);
            UIView *dim = a.view.superview.subviews.firstObject;
            printf("VALUE dimming %s bg %s\n", rect([dim convertRect:dim.bounds toView:w]).UTF8String, rgba(dim.backgroundColor).UTF8String);
            for (UIWindow *any in w.windowScene.windows) {
                printf("DUMP window %s level %.0f\n", NSStringFromClass(any.class).UTF8String, any.windowLevel);
                dump(any, any, 1);
            }
            fflush(stdout);
        });
    }];
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
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@end
@implementation AppDelegate
- (BOOL)application:(UIApplication *)application didFinishLaunchingWithOptions:(NSDictionary *)options {
    t0 = CFAbsoluteTimeGetCurrent();
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
