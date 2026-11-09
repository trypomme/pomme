// A small UIKit app with a scroll view, for tests/scroll.rs. Results are shown
// in text fields (the view dump prints their text), told apart by their tags.
#import <UIKit/UIKit.h>

// A row: counts taps (touches that end on it) and touches taken by scrolling.
@interface Row : UIView
@property(nonatomic, copy) void (^onTap)(void);
@property(nonatomic, copy) void (^onCancel)(void);
@end

@implementation Row
- (void)touchesEnded:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
    self.onTap();
}
- (void)touchesCancelled:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
    self.onCancel();
}
@end

@interface ViewController : UIViewController <UIScrollViewDelegate>
@property(nonatomic, strong) UIScrollView *scroll;
@property(nonatomic, strong) UITextField *events;
@property(nonatomic, strong) UITextField *position;
@property(nonatomic) int begins, scrolls, ends, decelerates, stops, taps, cancels;
@property(nonatomic) BOOL willDecelerate;
@end

@implementation ViewController
- (UITextField *)fieldAt:(CGFloat)y tag:(NSInteger)tag {
    UITextField *f = [[UITextField alloc] initWithFrame:CGRectMake(20, y, 350, 30)];
    f.tag = tag;
    [self.view addSubview:f];
    return f;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;
    // 400 pt tall, 40 rows of 50 pt: 2000 pt of content, scrolling to 1600.
    self.scroll = [[UIScrollView alloc] initWithFrame:CGRectMake(0, 100, 390, 400)];
    self.scroll.delegate = self;
    self.scroll.contentSize = CGSizeMake(390, 2000);
    [self.view addSubview:self.scroll];
    __weak ViewController *weak = self;
    for (int i = 0; i < 40; i++) {
        Row *row = [[Row alloc] initWithFrame:CGRectMake(0, 50 * i, 390, 50)];
        row.backgroundColor = i % 2 ? UIColor.lightGrayColor : UIColor.whiteColor;
        row.onTap = ^{ weak.taps++; [weak update]; };
        row.onCancel = ^{ weak.cancels++; [weak update]; };
        [self.scroll addSubview:row];
    }
    self.events = [self fieldAt:520 tag:100];
    self.position = [self fieldAt:560 tag:101];
    [self update];
}

- (void)update {
    self.events.text = [NSString stringWithFormat:@"begin %d end %d decelerate %d stop %d tap %d cancel %d", self.begins, self.ends, self.willDecelerate,
                                                  self.stops, self.taps, self.cancels];
    self.position.text = [NSString stringWithFormat:@"offset %.0f dragging %d decelerating %d scrolled %d", self.scroll.contentOffset.y,
                                                    self.scroll.isDragging, self.scroll.isDecelerating, self.scrolls > 0];
}

- (void)scrollViewDidScroll:(UIScrollView *)scrollView {
    self.scrolls++;
    [self update];
}

- (void)scrollViewWillBeginDragging:(UIScrollView *)scrollView {
    self.begins++;
    [self update];
}

// Land on a multiple of 100 pt.
- (void)scrollViewWillEndDragging:(UIScrollView *)scrollView withVelocity:(CGPoint)velocity targetContentOffset:(inout CGPoint *)target {
    target->y = round(target->y / 100) * 100;
}

- (void)scrollViewDidEndDragging:(UIScrollView *)scrollView willDecelerate:(BOOL)decelerate {
    self.ends++;
    self.willDecelerate = decelerate;
    [self update];
}

- (void)scrollViewWillBeginDecelerating:(UIScrollView *)scrollView {
    self.decelerates++;
}

- (void)scrollViewDidEndDecelerating:(UIScrollView *)scrollView {
    self.stops++;
    [self update];
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
