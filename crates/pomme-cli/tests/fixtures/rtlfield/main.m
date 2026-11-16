// Text fields with mixed Arabic and English, for tests/rtl.rs: where each
// field draws its text (its alignment) and where the caret goes at each
// position (`caretRectForPosition:`, in the field) and where it moves a step
// left, right, up or down from each (`VALUE fieldN.leftK`), in an app with an
// Arabic localization (`focus`: the third field edited; VARIANT=ar, en with scripts/swiftui/metrics-simulator.sh).
#import <UIKit/UIKit.h>

@interface Root : UIViewController
@end

@implementation Root
- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;
    NSArray *texts = @[ @"مرحبا Pomme", @"Pomme مرحبا", @"Hello" ];
    for (NSUInteger i = 0; i < texts.count; i++) {
        UITextField *f = [[UITextField alloc] initWithFrame:CGRectMake(20, 120 + 60 * i, 300, 40)];
        f.text = texts[i];
        f.borderStyle = UITextBorderStyleRoundedRect;
        f.tag = i + 1;
        [f addTarget:self action:@selector(changed:) forControlEvents:UIControlEventEditingChanged];
        [self.view addSubview:f];
    }
}
// (When typed into: the text, as tests/rtl.rs types with the arrow keys.)
- (void)changed:(UITextField *)f {
    printf("TYPED field%ld %s\n", (long)f.tag, f.text.UTF8String);
    fflush(stdout);
}
- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    for (NSInteger tag = 1; tag <= 3; tag++) {
        UITextField *f = [self.view viewWithTag:tag];
        printf("VALUE field%ld.textAlignment %ld\n", (long)tag, (long)f.textAlignment);
        CGRect tr = [f textRectForBounds:f.bounds];
        printf("METRIC field%ld.textRect %.2f %.2f %.2f %.2f\n", (long)tag, tr.origin.x, tr.origin.y, tr.size.width, tr.size.height);
        for (NSInteger k = 0; k <= (NSInteger)f.text.length; k++) {
            UITextPosition *p = [f positionFromPosition:f.beginningOfDocument offset:k];
            CGRect r = [f caretRectForPosition:p];
            printf("METRIC field%ld.caret%ld %.2f %.2f %.2f %.2f\n", (long)tag, (long)k, r.origin.x, r.origin.y, r.size.width, r.size.height);
            // Where the caret goes from here moving one step left, right,
            // up and down on screen (as the arrow keys move it).
            const char *names[] = {"right", "left", "up", "down"};
            for (NSInteger d = 0; d < 4; d++) {
                UITextPosition *q = [f positionFromPosition:p inDirection:(UITextLayoutDirection)(UITextLayoutDirectionRight + d) offset:1];
                printf("VALUE field%ld.%s%ld %ld\n", (long)tag, names[d], (long)k, q ? (long)[f offsetFromPosition:f.beginningOfDocument toPosition:q] : -1L);
            }
        }
    }
    fflush(stdout);
    // The `focus` case: "Hello" edited, its caret after "He" (for the caret's look).
    const char *v = getenv("PROBE_VARIANT");
    if (v && !strcmp(v, "focus")) {
        UITextField *f = [self.view viewWithTag:3];
        [f becomeFirstResponder];
        UITextPosition *p = [f positionFromPosition:f.beginningOfDocument offset:2];
        f.selectedTextRange = [f textRangeFromPosition:p toPosition:p];
    }
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
