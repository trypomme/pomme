// A small UIKit app with buttons, for tests/controls.rs: UIControl touch
// tracking and target-action. Each button logs what it hears into a text
// field (the view dump prints their text), told apart by their tags.
#import <UIKit/UIKit.h>

// A view that logs the touches that reach it (a disabled button passes them on).
@interface Catcher : UIView
@property(nonatomic, copy) void (^onEnded)(void);
@end

@implementation Catcher
- (void)touchesEnded:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
    self.onEnded();
}
@end

// A button that refuses to track: overriding beginTracking keeps its actions from firing.
@interface Gate : UIButton
@property(nonatomic, copy) void (^log)(NSString *);
@end

@implementation Gate
- (BOOL)beginTrackingWithTouch:(UITouch *)touch withEvent:(UIEvent *)event {
    self.log([NSString stringWithFormat:@"begin(%.0f)", [touch locationInView:self].x]);
    return NO;
}
- (void)endTrackingWithTouch:(UITouch *)touch withEvent:(UIEvent *)event {
    self.log(@"end");
}
@end

// Lists let a drag take the touch from a control (UITableView does this).
@interface ListLike : UIScrollView
@end

@implementation ListLike
- (BOOL)touchesShouldCancelInContentView:(UIView *)view {
    return YES;
}
@end

@interface ViewController : UIViewController <UIScrollViewDelegate>
@property(nonatomic, strong) UITextField *tapLog, *disabledLog, *chainLog, *gateLog, *panLog, *scrollLog, *listLog;
@end

@implementation ViewController
- (UITextField *)fieldAt:(CGFloat)y tag:(NSInteger)tag {
    UITextField *f = [[UITextField alloc] initWithFrame:CGRectMake(20, y, 350, 30)];
    f.tag = tag;
    f.text = @"";
    [self.view addSubview:f];
    return f;
}

- (void)log:(NSString *)s to:(UITextField *)f {
    f.text = f.text.length ? [f.text stringByAppendingFormat:@" %@", s] : s;
}

- (UIButton *)button:(NSString *)title at:(CGFloat)y in:(UIView *)parent {
    UIButton *b = [UIButton buttonWithType:UIButtonTypeSystem];
    b.frame = CGRectMake(40, y, 150, 50);
    [b setTitle:title forState:UIControlStateNormal];
    [parent addSubview:b];
    return b;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;

    // Tap target: logs each event with the control's state at the time.
    UIButton *tap = [self button:@"Tap" at:100 in:self.view];
    [tap addTarget:self action:@selector(down:) forControlEvents:UIControlEventTouchDown];
    [tap addTarget:self action:@selector(exit:) forControlEvents:UIControlEventTouchDragExit];
    [tap addTarget:self action:@selector(enter:) forControlEvents:UIControlEventTouchDragEnter];
    [tap addTarget:self action:@selector(upInside:forEvent:) forControlEvents:UIControlEventTouchUpInside];
    [tap addTarget:self action:@selector(upOutside) forControlEvents:UIControlEventTouchUpOutside];
    [tap addTarget:self action:@selector(primary:) forControlEvents:UIControlEventPrimaryActionTriggered];
    [tap addTarget:self action:@selector(cancel:) forControlEvents:UIControlEventTouchCancel];

    // Disabled: no actions; the touches go on to the view behind it.
    Catcher *catcher = [[Catcher alloc] initWithFrame:CGRectMake(0, 180, 390, 80)];
    __weak ViewController *weak = self;
    catcher.onEnded = ^{ [weak log:@"container" to:weak.disabledLog]; };
    [self.view addSubview:catcher];
    UIButton *disabled = [self button:@"Off" at:10 in:catcher];
    [disabled addTarget:self action:@selector(disabledTapped:) forControlEvents:UIControlEventTouchUpInside];
    disabled.enabled = NO;

    // No target: the action goes up the responder chain to this controller.
    UIButton *chain = [self button:@"Chain" at:280 in:self.view];
    [chain addTarget:nil action:@selector(chainAction:) forControlEvents:UIControlEventTouchUpInside];

    // A subclass that won't track.
    Gate *gate = [Gate buttonWithType:UIButtonTypeSystem];
    gate.frame = CGRectMake(40, 360, 150, 50);
    gate.log = ^(NSString *s) { [weak log:s to:weak.gateLog]; };
    [gate addTarget:self action:@selector(gateAction:) forControlEvents:UIControlEventTouchDown | UIControlEventTouchUpInside];
    [self.view addSubview:gate];

    // A pan on the container takes a dragging touch: the button's tracking is cancelled.
    UIView *panned = [[UIView alloc] initWithFrame:CGRectMake(0, 430, 390, 80)];
    [panned addGestureRecognizer:[[UIPanGestureRecognizer alloc] initWithTarget:self action:@selector(panned:)]];
    [self.view addSubview:panned];
    UIButton *inPan = [self button:@"Pan" at:10 in:panned];
    [inPan addTarget:self action:@selector(panDown:) forControlEvents:UIControlEventTouchDown];
    [inPan addTarget:self action:@selector(panCancel:) forControlEvents:UIControlEventTouchCancel];
    [inPan addTarget:self action:@selector(panUp:) forControlEvents:UIControlEventTouchUpInside | UIControlEventTouchUpOutside];

    // Buttons in scroll views (on the right): a plain one keeps a control's
    // touch once the control has it; a list-like one takes it.
    UIScrollView *plain = [self scrollAt:100 class:UIScrollView.class];
    UIButton *inPlain = [self button:@"Plain" at:20 in:plain];
    [inPlain addTarget:self action:@selector(scrollDown:) forControlEvents:UIControlEventTouchDown];
    [inPlain addTarget:self action:@selector(scrollUp:) forControlEvents:UIControlEventTouchUpInside];
    [inPlain addTarget:self action:@selector(scrollCancel:) forControlEvents:UIControlEventTouchCancel];
    UIScrollView *list = [self scrollAt:320 class:ListLike.class];
    UIButton *inList = [self button:@"List" at:20 in:list];
    [inList addTarget:self action:@selector(listDown:) forControlEvents:UIControlEventTouchDown];
    [inList addTarget:self action:@selector(listUp:) forControlEvents:UIControlEventTouchUpInside];
    [inList addTarget:self action:@selector(listCancel:) forControlEvents:UIControlEventTouchCancel];

    self.tapLog = [self fieldAt:560 tag:100];
    self.disabledLog = [self fieldAt:600 tag:101];
    self.chainLog = [self fieldAt:640 tag:102];
    self.gateLog = [self fieldAt:680 tag:103];
    self.panLog = [self fieldAt:720 tag:104];
    self.scrollLog = [self fieldAt:760 tag:105];
    self.listLog = [self fieldAt:800 tag:106];
}

- (UIScrollView *)scrollAt:(CGFloat)y class:(Class)cls {
    UIScrollView *s = [[cls alloc] initWithFrame:CGRectMake(200, y, 190, 200)];
    s.contentSize = CGSizeMake(190, 1000);
    s.delegate = self;
    [self.view addSubview:s];
    return s;
}

- (UITextField *)logFor:(UIScrollView *)s {
    return [s isKindOfClass:ListLike.class] ? self.listLog : self.scrollLog;
}
- (void)scrollViewWillBeginDragging:(UIScrollView *)s {
    [self log:@"drag" to:[self logFor:s]];
}
- (void)scrollDown:(UIButton *)b { [self log:@"down" to:self.scrollLog]; }
- (void)scrollUp:(UIButton *)b { [self log:@"up" to:self.scrollLog]; }
- (void)scrollCancel:(UIButton *)b { [self log:@"cancel" to:self.scrollLog]; }
- (void)listDown:(UIButton *)b { [self log:@"down" to:self.listLog]; }
- (void)listUp:(UIButton *)b { [self log:@"up" to:self.listLog]; }
- (void)listCancel:(UIButton *)b { [self log:@"cancel" to:self.listLog]; }

- (void)down:(UIButton *)b {
    [self log:[NSString stringWithFormat:@"down(h%d t%d)", b.highlighted, b.tracking] to:self.tapLog];
}
- (void)exit:(UIButton *)b {
    [self log:[NSString stringWithFormat:@"exit(h%d)", b.highlighted] to:self.tapLog];
}
- (void)enter:(UIButton *)b {
    [self log:@"enter" to:self.tapLog];
}
- (void)upInside:(UIButton *)b forEvent:(UIEvent *)event {
    [self log:[NSString stringWithFormat:@"up(h%d in%d touches%lu)", b.highlighted, b.touchInside, (unsigned long)event.allTouches.count] to:self.tapLog];
}
- (void)upOutside {
    [self log:@"upOutside" to:self.tapLog];
}
- (void)primary:(UIButton *)b {
    [self log:@"primary" to:self.tapLog];
}
- (void)cancel:(UIButton *)b {
    [self log:@"cancel" to:self.tapLog];
}
- (void)disabledTapped:(UIButton *)b {
    [self log:@"action" to:self.disabledLog];
}
- (void)chainAction:(UIButton *)sender {
    [self log:[NSString stringWithFormat:@"chain %@", sender.currentTitle] to:self.chainLog];
}
- (void)gateAction:(UIButton *)b {
    [self log:@"action" to:self.gateLog];
}
- (void)panned:(UIPanGestureRecognizer *)g {
    if (g.state == UIGestureRecognizerStateBegan) [self log:@"pan" to:self.panLog];
}
- (void)panDown:(UIButton *)b {
    [self log:@"down" to:self.panLog];
}
- (void)panCancel:(UIButton *)b {
    [self log:[NSString stringWithFormat:@"cancel(h%d)", b.highlighted] to:self.panLog];
}
- (void)panUp:(UIButton *)b {
    [self log:@"up" to:self.panLog];
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
