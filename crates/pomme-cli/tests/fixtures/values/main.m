// A UIKit app with switches, sliders and segmented controls, for
// tests/values.rs. Each logs its ValueChanged events into a text field (the
// view dump prints their text), told apart by their tags.
#import <UIKit/UIKit.h>

@interface ViewController : UIViewController
@property(nonatomic, strong) UISwitch *off, *on;
@property(nonatomic, strong) UISlider *slider, *settled;
@property(nonatomic, strong) UISegmentedControl *segments, *momentary;
@property(nonatomic, strong) UITextField *switchLog, *sliderLog, *settledLog, *segmentLog, *momentaryLog, *sizeLog;
@property(nonatomic) int sliderChanges, settledChanges;
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

- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;

    // Switches: one off, one on. Asked for 100×100, they're 51×31.
    self.off = [[UISwitch alloc] initWithFrame:CGRectMake(40, 100, 100, 100)];
    [self.off addTarget:self action:@selector(switched:) forControlEvents:UIControlEventValueChanged];
    [self.view addSubview:self.off];
    self.on = [UISwitch new];
    self.on.frame = CGRectMake(40, 160, 0, 0);
    self.on.on = YES;
    [self.on addTarget:self action:@selector(switched:) forControlEvents:UIControlEventValueChanged];
    [self.view addSubview:self.on];

    // Sliders over 0…100 from 25: one continuous, one reporting only when let go.
    self.slider = [self sliderAt:240];
    self.settled = [self sliderAt:300];
    self.settled.continuous = NO;

    // Segmented controls: three equal 100-pt segments, the third disabled; and a momentary one.
    self.segments = [[UISegmentedControl alloc] initWithItems:@[ @"One", @"Two", @"Three" ]];
    self.segments.frame = CGRectMake(40, 380, 300, 32);
    self.segments.selectedSegmentIndex = 0;
    [self.segments setEnabled:NO forSegmentAtIndex:2];
    [self.segments addTarget:self action:@selector(segmented:) forControlEvents:UIControlEventValueChanged];
    [self.view addSubview:self.segments];
    self.momentary = [[UISegmentedControl alloc] initWithItems:@[ @"A", @"B" ]];
    self.momentary.frame = CGRectMake(40, 440, 300, 32);
    self.momentary.momentary = YES;
    [self.momentary addTarget:self action:@selector(segmented:) forControlEvents:UIControlEventValueChanged];
    [self.view addSubview:self.momentary];

    self.switchLog = [self fieldAt:560 tag:200];
    self.sliderLog = [self fieldAt:600 tag:201];
    self.settledLog = [self fieldAt:640 tag:202];
    self.segmentLog = [self fieldAt:680 tag:203];
    self.momentaryLog = [self fieldAt:720 tag:204];
    self.sizeLog = [self fieldAt:760 tag:205];

    // Natural sizes (a fresh segmented control is its own size).
    UISegmentedControl *fresh = [[UISegmentedControl alloc] initWithItems:@[ @"One", @"Two" ]];
    [fresh insertSegmentWithTitle:@"Zero" atIndex:0 animated:NO];
    [fresh removeSegmentAtIndex:2 animated:NO];
    self.sizeLog.text = [NSString stringWithFormat:@"switch %@ on %d slider %@ fresh %@ %lu %@,%@ sel %ld", NSStringFromCGSize(self.on.frame.size), self.on.isOn,
                                                   NSStringFromCGSize([self.slider sizeThatFits:CGSizeMake(0, 0)]), NSStringFromCGSize(fresh.frame.size),
                                                   (unsigned long)fresh.numberOfSegments, [fresh titleForSegmentAtIndex:0], [fresh titleForSegmentAtIndex:1],
                                                   (long)fresh.selectedSegmentIndex];
}

- (UISlider *)sliderAt:(CGFloat)y {
    UISlider *s = [[UISlider alloc] initWithFrame:CGRectMake(40, y, 300, 31)];
    s.minimumValue = 0;
    s.maximumValue = 100;
    s.value = 25;
    [s addTarget:self action:@selector(slid:) forControlEvents:UIControlEventValueChanged];
    [self.view addSubview:s];
    return s;
}

- (void)switched:(UISwitch *)s {
    [self log:[NSString stringWithFormat:@"%@ %d", s == self.off ? @"off-switch" : @"on-switch", s.isOn] to:self.switchLog];
}

- (void)slid:(UISlider *)s {
    if (s == self.slider) {
        self.sliderChanges++;
        self.sliderLog.text = [NSString stringWithFormat:@"changes %d value %.0f tracking %d", self.sliderChanges, s.value, s.tracking];
    } else {
        self.settledChanges++;
        self.settledLog.text = [NSString stringWithFormat:@"changes %d value %.0f", self.settledChanges, s.value];
    }
}

- (void)segmented:(UISegmentedControl *)c {
    [self log:[NSString stringWithFormat:@"%ld", (long)c.selectedSegmentIndex] to:c == self.segments ? self.segmentLog : self.momentaryLog];
    // A momentary control lets go of the segment once it has reported it.
    if (c == self.momentary) [self performSelector:@selector(afterMomentary) withObject:nil afterDelay:0];
}

- (void)afterMomentary {
    [self log:[NSString stringWithFormat:@"then %ld", (long)self.momentary.selectedSegmentIndex] to:self.momentaryLog];
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
