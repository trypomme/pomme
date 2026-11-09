// Dragging sheets, for tests/sheetdrag.rs. On a red root screen: at 0.5 s a
// green sheet (large) is presented, which the test drags down to dismiss; at
// 3 s a blue one with medium and large detents, which it drags up to large,
// then a little down and back (dismissed at 6 s); at 6.5 s a yellow one that's modal in
// presentation, which refuses to be dragged away. Label 1 lists what the
// sheets' delegate heard.
#import <UIKit/UIKit.h>

@interface Listener : NSObject <UISheetPresentationControllerDelegate>
@property(nonatomic, weak) UILabel *label;
@end

@implementation Listener
- (void)note:(NSString *)s {
    self.label.text = [self.label.text stringByAppendingFormat:@"%@ ", s];
}
- (void)presentationControllerWillDismiss:(UIPresentationController *)p { [self note:@"will"]; }
- (void)presentationControllerDidDismiss:(UIPresentationController *)p { [self note:@"did"]; }
- (void)presentationControllerDidAttemptToDismiss:(UIPresentationController *)p { [self note:@"attempt"]; }
- (void)sheetPresentationControllerDidChangeSelectedDetentIdentifier:(UISheetPresentationController *)s {
    [self note:[s.selectedDetentIdentifier isEqualToString:UISheetPresentationControllerDetentIdentifierLarge] ? @"large" : @"medium"];
}
@end

static UIViewController *colored(UIColor *c) {
    UIViewController *vc = [UIViewController new];
    vc.view.backgroundColor = c;
    return vc;
}

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@property(nonatomic, strong) Listener *listener;
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)options {
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    UIViewController *root = colored(UIColor.redColor);
    self.window.rootViewController = root;
    [self.window makeKeyAndVisible];
    UILabel *log = [[UILabel alloc] initWithFrame:CGRectMake(20, 600, 350, 30)];
    log.tag = 1;
    log.text = @"";
    [root.view addSubview:log];
    self.listener = [Listener new];
    self.listener.label = log;
    Listener *listener = self.listener;
    UIViewController *green = colored(UIColor.greenColor);
    green.presentationController.delegate = listener;
    UIViewController *blue = colored(UIColor.blueColor);
    blue.sheetPresentationController.detents = @[UISheetPresentationControllerDetent.mediumDetent, UISheetPresentationControllerDetent.largeDetent];
    blue.sheetPresentationController.delegate = listener;
    UIViewController *yellow = colored(UIColor.yellowColor);
    yellow.modalInPresentation = YES;
    yellow.presentationController.delegate = listener;
    [NSTimer scheduledTimerWithTimeInterval:0.5 repeats:NO block:^(NSTimer *t) { [root presentViewController:green animated:YES completion:nil]; }];
    [NSTimer scheduledTimerWithTimeInterval:3.0 repeats:NO block:^(NSTimer *t) { [root presentViewController:blue animated:YES completion:nil]; }];
    [NSTimer scheduledTimerWithTimeInterval:6.0 repeats:NO block:^(NSTimer *t) { [root dismissViewControllerAnimated:NO completion:nil]; }];
    [NSTimer scheduledTimerWithTimeInterval:6.5 repeats:NO block:^(NSTimer *t) { [root presentViewController:yellow animated:YES completion:nil]; }];
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
