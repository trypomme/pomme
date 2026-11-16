// Table views, for tests/table_view.rs. PROBE_VARIANT picks the case:
// `plain`, `grouped`, `insetGrouped`: two sections of three rows with header
// and footer titles; `styles`: a cell of each standard style with an image,
// text, detail text and a disclosure indicator; `long`: 100 rows, scrolled
// to the bottom after a second; `groupedBare`: grouped, no titles;
// `controller`: a UITableViewController (plain) as the window's root, its
// table the whole screen (its frame, insets and offset printed too). In
// `plain`, row 0.1 is then selected and its cell listed again (`selected`). The table sits at (0, 100), 390 × 700.
// Printed (scripts/swiftui/metrics-simulator.sh, VARIANT=<case>):
// `METRIC name x y width height` for the table's rects (rows, headers,
// footers, in the table's content), `VIEW row class x y w h text` for each
// visible cell's subviews (in the cell), and `VALUE name value`.
#import <UIKit/UIKit.h>

static NSString *variant(void) {
    const char *v = getenv("PROBE_VARIANT");
    return v && *v ? @(v) : @"plain";
}

static void dump(NSString *name, UIView *v, UIView *root) {
    CGRect r = [v convertRect:v.bounds toView:root];
    NSString *text = [v isKindOfClass:UILabel.class] ? ((UILabel *)v).text : @"";
    if (!v.hidden && v.alpha > 0.01 && r.size.width > 0) {
        // (Then its fill, as 0-255 RGBA, and its corner radius, when it has them.)
        NSString *extra = @"";
        if (v.backgroundColor) {
            CGFloat c[4] = {0};
            [[v.backgroundColor resolvedColorWithTraitCollection:v.traitCollection] getRed:&c[0] green:&c[1] blue:&c[2] alpha:&c[3]];
            extra = [extra stringByAppendingFormat:@" bg=%d,%d,%d,%d", (int)lround(c[0] * 255), (int)lround(c[1] * 255), (int)lround(c[2] * 255), (int)lround(c[3] * 255)];
        }
        if (v.layer.cornerRadius > 0) extra = [extra stringByAppendingFormat:@" radius=%.2f corners=%lu", v.layer.cornerRadius, (unsigned long)v.layer.maskedCorners];
        if ([v isKindOfClass:UILabel.class]) {
            UILabel *l = (UILabel *)v;
            CGFloat c[4] = {0};
            [[l.textColor resolvedColorWithTraitCollection:v.traitCollection] getRed:&c[0] green:&c[1] blue:&c[2] alpha:&c[3]];
            extra = [extra stringByAppendingFormat:@" font=%.1f/%.2f color=%d,%d,%d,%d", l.font.pointSize, [[l.font.fontDescriptor objectForKey:UIFontDescriptorTraitsAttribute][UIFontWeightTrait] doubleValue], (int)lround(c[0] * 255), (int)lround(c[1] * 255), (int)lround(c[2] * 255), (int)lround(c[3] * 255)];
        }
        printf("VIEW %s %s %.2f %.2f %.2f %.2f %s%s\n", name.UTF8String, NSStringFromClass(v.class).UTF8String, r.origin.x, r.origin.y, r.size.width, r.size.height, (text ?: @"").UTF8String, extra.UTF8String);
    }
    for (UIView *s in v.subviews) dump(name, s, root);
}

static void metric(NSString *name, CGRect r) {
    printf("METRIC %s %.2f %.2f %.2f %.2f\n", name.UTF8String, r.origin.x, r.origin.y, r.size.width, r.size.height);
}

@interface Root : UIViewController <UITableViewDataSource, UITableViewDelegate>
@property(nonatomic, strong) UITableView *table;
@property(nonatomic) NSInteger made;
@property(nonatomic, strong) NSHashTable *cells;
@end

@implementation Root
- (BOOL)isLong { return [variant() isEqual:@"long"]; }
- (BOOL)isStyles { return [variant() isEqual:@"styles"]; }
- (BOOL)isBare { return [variant() isEqual:@"groupedBare"]; }

- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;
    NSString *v = variant();
    UITableViewStyle style = [v hasPrefix:@"grouped"] ? UITableViewStyleGrouped : [v isEqual:@"insetGrouped"] ? UITableViewStyleInsetGrouped : UITableViewStylePlain;
    self.table = [[UITableView alloc] initWithFrame:CGRectMake(0, 100, 390, 700) style:style];
    self.table.dataSource = self;
    self.table.delegate = self;
    [self.table registerClass:UITableViewCell.class forCellReuseIdentifier:@"row"];
    [self.view addSubview:self.table];
}

- (NSInteger)numberOfSectionsInTableView:(UITableView *)t {
    return self.isLong || self.isStyles ? 1 : 2;
}
- (NSInteger)tableView:(UITableView *)t numberOfRowsInSection:(NSInteger)s {
    return self.isLong ? 100 : self.isStyles ? 4 : 3;
}
- (NSString *)tableView:(UITableView *)t titleForHeaderInSection:(NSInteger)s {
    return self.isLong || self.isStyles || self.isBare ? nil : [NSString stringWithFormat:@"Section %ld", (long)s];
}
- (NSString *)tableView:(UITableView *)t titleForFooterInSection:(NSInteger)s {
    return self.isLong || self.isStyles || self.isBare ? nil : @"Footer";
}
- (UITableViewCell *)tableView:(UITableView *)t cellForRowAtIndexPath:(NSIndexPath *)p {
    UITableViewCell *cell;
    if (self.isStyles) {
        cell = [[UITableViewCell alloc] initWithStyle:(UITableViewCellStyle)p.row reuseIdentifier:nil];
        cell.detailTextLabel.text = @"Detail";
        cell.imageView.image = [UIImage systemImageNamed:@"star.fill"];
        cell.accessoryType = UITableViewCellAccessoryDisclosureIndicator;
    } else {
        cell = [t dequeueReusableCellWithIdentifier:@"row" forIndexPath:p];
    }
    cell.textLabel.text = [NSString stringWithFormat:@"Row %ld", (long)p.row];
    self.made++;
    if (!self.cells) self.cells = [NSHashTable weakObjectsHashTable];
    [self.cells addObject:cell];
    return cell;
}

- (void)tableView:(UITableView *)t didSelectRowAtIndexPath:(NSIndexPath *)p {
    printf("VALUE didSelect %ld %ld\n", (long)p.section, (long)p.row);
    fflush(stdout);
}

- (void)report:(NSString *)when {
    UITableView *t = self.table;
    printf("VALUE %s.contentSize %.2f %.2f\n", when.UTF8String, t.contentSize.width, t.contentSize.height);
    printf("VALUE %s.contentOffset %.2f\n", when.UTF8String, t.contentOffset.y);
    printf("VALUE %s.cellsAsked %ld\n", when.UTF8String, (long)self.made);
    printf("VALUE %s.distinctCells %lu\n", when.UTF8String, (unsigned long)self.cells.allObjects.count);
    NSMutableArray *rows = [NSMutableArray array];
    for (NSIndexPath *p in t.indexPathsForVisibleRows) [rows addObject:[NSString stringWithFormat:@"%ld.%ld", (long)p.section, (long)p.row]];
    printf("VALUE %s.visibleRows %s\n", when.UTF8String, [rows componentsJoinedByString:@","].UTF8String);
    fflush(stdout);
}

- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    UITableView *t = self.table;
    printf("VALUE rowHeight %.2f\n", t.rowHeight);
    printf("VALUE estimatedRowHeight %.2f\n", t.estimatedRowHeight);
    printf("VALUE sectionHeaderHeight %.2f\n", t.sectionHeaderHeight);
    printf("VALUE sectionFooterHeight %.2f\n", t.sectionFooterHeight);
    printf("VALUE separatorInset %.2f %.2f %.2f %.2f\n", t.separatorInset.top, t.separatorInset.left, t.separatorInset.bottom, t.separatorInset.right);
    printf("VALUE separatorStyle %ld\n", (long)t.separatorStyle);
    CGFloat c[4] = {0};
    [[t.backgroundColor resolvedColorWithTraitCollection:t.traitCollection] getRed:&c[0] green:&c[1] blue:&c[2] alpha:&c[3]];
    printf("VALUE backgroundColor %d,%d,%d,%d\n", (int)lround(c[0] * 255), (int)lround(c[1] * 255), (int)lround(c[2] * 255), (int)lround(c[3] * 255));
    [[t.separatorColor resolvedColorWithTraitCollection:t.traitCollection] getRed:&c[0] green:&c[1] blue:&c[2] alpha:&c[3]];
    printf("VALUE separatorColor %d,%d,%d,%d\n", (int)lround(c[0] * 255), (int)lround(c[1] * 255), (int)lround(c[2] * 255), (int)lround(c[3] * 255));
    for (NSInteger s = 0; s < t.numberOfSections; s++) {
        metric([NSString stringWithFormat:@"section%ld", (long)s], [t rectForSection:s]);
        metric([NSString stringWithFormat:@"header%ld", (long)s], [t rectForHeaderInSection:s]);
        metric([NSString stringWithFormat:@"footer%ld", (long)s], [t rectForFooterInSection:s]);
        NSInteger n = MIN([t numberOfRowsInSection:s], 6);
        for (NSInteger r = 0; r < n; r++) {
            metric([NSString stringWithFormat:@"row%ld.%ld", (long)s, (long)r], [t rectForRowAtIndexPath:[NSIndexPath indexPathForRow:r inSection:s]]);
        }
        UITableViewHeaderFooterView *h = [t headerViewForSection:s];
        if (h) dump([NSString stringWithFormat:@"header%ld", (long)s], h, h);
        UITableViewHeaderFooterView *f = [t footerViewForSection:s];
        if (f) dump([NSString stringWithFormat:@"footer%ld", (long)s], f, f);
    }
    for (UITableViewCell *c in t.visibleCells) {
        NSIndexPath *p = [t indexPathForCell:c];
        if (p.row < 6) dump([NSString stringWithFormat:@"cell%ld.%ld", (long)p.section, (long)p.row], c, c);
    }
    [self report:@"start"];
    if ([variant() isEqual:@"plain"]) {
        NSIndexPath *p = [NSIndexPath indexPathForRow:1 inSection:0];
        [t selectRowAtIndexPath:p animated:NO scrollPosition:UITableViewScrollPositionNone];
        dump(@"selected", [t cellForRowAtIndexPath:p], [t cellForRowAtIndexPath:p]);
        fflush(stdout);
    }
    if (self.isLong) {
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC), dispatch_get_main_queue(), ^{
            [t setContentOffset:CGPointMake(0, t.contentSize.height - t.bounds.size.height + t.adjustedContentInset.bottom) animated:NO];
            [t layoutIfNeeded];
            [self report:@"bottom"];
        });
    }
}
@end

@interface Controller : UITableViewController
@end

@implementation Controller
- (void)viewDidLoad {
    [super viewDidLoad];
    [self.tableView registerClass:UITableViewCell.class forCellReuseIdentifier:@"row"];
}
- (NSInteger)numberOfSectionsInTableView:(UITableView *)t {
    return 2;
}
- (NSInteger)tableView:(UITableView *)t numberOfRowsInSection:(NSInteger)s {
    return 3;
}
- (NSString *)tableView:(UITableView *)t titleForHeaderInSection:(NSInteger)s {
    return [NSString stringWithFormat:@"Section %ld", (long)s];
}
- (UITableViewCell *)tableView:(UITableView *)t cellForRowAtIndexPath:(NSIndexPath *)p {
    UITableViewCell *cell = [t dequeueReusableCellWithIdentifier:@"row" forIndexPath:p];
    cell.textLabel.text = [NSString stringWithFormat:@"Row %ld", (long)p.row];
    return cell;
}
- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    UITableView *t = self.tableView;
    printf("VALUE controller.isTableView %d\n", self.view == t);
    printf("VALUE controller.dataSource %d\n", t.dataSource == self);
    metric(@"controller.table", t.frame);
    UIEdgeInsets a = t.adjustedContentInset;
    printf("VALUE controller.adjustedContentInset %.2f %.2f %.2f %.2f\n", a.top, a.left, a.bottom, a.right);
    printf("VALUE controller.contentOffset %.2f\n", t.contentOffset.y);
    for (NSInteger s = 0; s < 2; s++) {
        metric([NSString stringWithFormat:@"header%ld", (long)s], [t rectForHeaderInSection:s]);
        for (NSInteger r = 0; r < 3; r++) metric([NSString stringWithFormat:@"row%ld.%ld", (long)s, (long)r], [t rectForRowAtIndexPath:[NSIndexPath indexPathForRow:r inSection:s]]);
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
    self.window.rootViewController = [variant() isEqual:@"controller"] ? [[Controller alloc] initWithStyle:UITableViewStylePlain] : [Root new];
    [self.window makeKeyAndVisible];
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
