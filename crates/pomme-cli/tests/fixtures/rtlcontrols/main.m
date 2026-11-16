// System controls in a right-to-left layout, for tests/rtl.rs: a switch
// off and on, a slider at a quarter, a segmented control, and a table cell
// with an image, text and a disclosure indicator. Each control's subviews
// are listed (`VIEW control class x y width height text`, in the control).
// scripts/swiftui/metrics-simulator.sh records it (VARIANT=<language>).
#import <UIKit/UIKit.h>

static void dump(NSString *name, UIView *v, UIView *root) {
    CGRect r = [v convertRect:v.bounds toView:root];
    NSString *text = [v isKindOfClass:UILabel.class] ? ((UILabel *)v).text : @"";
    if (!v.hidden && v.alpha > 0.01 && r.size.width > 0) {
        printf("VIEW %s %s %.2f %.2f %.2f %.2f %s\n", name.UTF8String, NSStringFromClass(v.class).UTF8String, r.origin.x, r.origin.y, r.size.width, r.size.height, (text ?: @"").UTF8String);
    }
    for (UIView *s in v.subviews) dump(name, s, root);
}

@interface Root : UIViewController <UITableViewDataSource>
@property(nonatomic, strong) NSDictionary<NSString *, UIView *> *controls;
@end

@implementation Root
- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;
    UISwitch *off = [[UISwitch alloc] initWithFrame:CGRectMake(40, 100, 0, 0)];
    UISwitch *on = [[UISwitch alloc] initWithFrame:CGRectMake(40, 150, 0, 0)];
    on.on = YES;
    UISlider *slider = [[UISlider alloc] initWithFrame:CGRectMake(40, 200, 300, 30)];
    slider.value = 0.25;
    UISegmentedControl *seg = [[UISegmentedControl alloc] initWithItems:@[ @"One", @"Two", @"Three" ]];
    seg.frame = CGRectMake(40, 250, 300, 32);
    seg.selectedSegmentIndex = 0;
    UITableView *table = [[UITableView alloc] initWithFrame:CGRectMake(0, 320, 390, 200) style:UITableViewStylePlain];
    table.dataSource = self;
    for (UIView *v in @[ off, on, slider, seg, table ]) [self.view addSubview:v];

    // Images: red on the left, blue on the right, as drawn and as flipped for
    // right to left; and two symbols, one that mirrors and one that doesn't.
    UIImage *halves = [[[UIGraphicsImageRenderer alloc] initWithSize:CGSizeMake(40, 40)] imageWithActions:^(UIGraphicsImageRendererContext *c) {
        [UIColor.redColor setFill];
        UIRectFill(CGRectMake(0, 0, 20, 40));
        [UIColor.blueColor setFill];
        UIRectFill(CGRectMake(20, 0, 20, 40));
    }];
    NSArray *images = @[ halves, halves.imageFlippedForRightToLeftLayoutDirection, [UIImage systemImageNamed:@"chevron.forward"], [UIImage systemImageNamed:@"chevron.right"] ];
    NSArray *imageNames = @[ @"image.plain", @"image.flipped", @"symbol.chevronForward", @"symbol.chevronRight" ];
    for (NSUInteger i = 0; i < images.count; i++) {
        UIImageView *iv = [[UIImageView alloc] initWithImage:images[i]];
        iv.frame = CGRectMake(40 + 80 * i, 560, 40, 40);
        iv.contentMode = UIViewContentModeScaleAspectFit;
        iv.tintColor = UIColor.blackColor;
        [self.view addSubview:iv];
        printf("VALUE %s.flips %d\n", [imageNames[i] UTF8String], [images[i] flipsForRightToLeftLayoutDirection]);
    }
    fflush(stdout);
    self.controls = @{ @"switchOff" : off, @"switchOn" : on, @"slider" : slider, @"segmented" : seg, @"table" : table };
}
- (NSInteger)tableView:(UITableView *)t numberOfRowsInSection:(NSInteger)s {
    return 1;
}
- (UITableViewCell *)tableView:(UITableView *)t cellForRowAtIndexPath:(NSIndexPath *)p {
    UITableViewCell *cell = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleDefault reuseIdentifier:nil];
    cell.textLabel.text = @"Row";
    cell.imageView.image = [UIImage systemImageNamed:@"star.fill"];
    cell.accessoryType = UITableViewCellAccessoryDisclosureIndicator;
    return cell;
}
- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC), dispatch_get_main_queue(), ^{
        for (NSString *name in [self.controls.allKeys sortedArrayUsingSelector:@selector(compare:)]) {
            UIView *c = self.controls[name];
            CGRect r = c.frame;
            printf("METRIC %s %.2f %.2f %.2f %.2f\n", name.UTF8String, r.origin.x, r.origin.y, r.size.width, r.size.height);
            dump(name, c, c);
        }
        fflush(stdout);
    });
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
