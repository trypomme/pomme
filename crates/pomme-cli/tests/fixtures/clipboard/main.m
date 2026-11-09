// A small UIKit app that uses the clipboard, for tests/clipboard.rs. Each
// result is shown in a text field (the view dump prints its text), told
// apart by its tag (100, 101, …).
#import <UIKit/UIKit.h>

static NSString *show(NSString *s) {
    return s ? s : @"(nil)";
}

@interface ViewController : UIViewController
@property(nonatomic, strong) NSMutableArray<UITextField *> *fields;
@end

@implementation ViewController
- (UITextField *)field:(NSInteger)i {
    return self.fields[i];
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.fields = [NSMutableArray new];
    for (NSInteger i = 0; i < 7; i++) {
        UITextField *f = [[UITextField alloc] initWithFrame:CGRectMake(20, 80 + 60 * i, 350, 44)];
        f.tag = 100 + i;
        [self.view addSubview:f];
        [self.fields addObject:f];
    }
}

- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    UIPasteboard *pb = UIPasteboard.generalPasteboard;
    // 100: whether there's text (never asks), and what it is (another app's: asks).
    [self field:0].text = [NSString stringWithFormat:@"has %d: %@", pb.hasStrings, show(pb.string)];
    // 101: the app copies, then reads its own copy back (doesn't ask).
    NSInteger before = pb.changeCount;
    pb.string = @"copied in the app";
    [self field:1].text = [NSString stringWithFormat:@"%@ (+%ld)", show(pb.string), (long)(pb.changeCount - before)];
    // 102: cut from a text field.
    UITextField *cut = [self field:2];
    cut.text = @"cut me";
    [cut selectAll:nil];
    [cut cut:nil];
    // 103: where --paste goes.
    [[self field:3] becomeFirstResponder];
    [self performSelector:@selector(soonAfterPaste) withObject:nil afterDelay:2.5];
    [self performSelector:@selector(later) withObject:nil afterDelay:4.0];
}

// 104: right after the user pasted (at 2 s): reading doesn't ask.
- (void)soonAfterPaste {
    [self field:4].text = show(UIPasteboard.generalPasteboard.string);
}

// 105: well after: another app's text asks again. 106: the URL form.
- (void)later {
    [self field:5].text = show(UIPasteboard.generalPasteboard.string);
    [self field:6].text = show(UIPasteboard.generalPasteboard.URL.absoluteString);
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
