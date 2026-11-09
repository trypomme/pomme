// A small UIKit app with text views, for tests/textview.rs. Results are shown
// in text fields (the view dump prints their text), told apart by their tags.
#import <UIKit/UIKit.h>

@interface ViewController : UIViewController <UITextViewDelegate>
@property(nonatomic, strong) UITextView *notes;
@property(nonatomic, strong) UITextView *readOnly;
@property(nonatomic, strong) UITextView *sized;
@property(nonatomic, strong) UITextField *log;
@property(nonatomic, strong) UITextField *scroll;
@property(nonatomic, strong) UITextField *other;
@property(nonatomic) int begins, changes, selections, ends, scrolls;
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
    // 100 pt tall: four 17 pt lines fit (with the 8 pt insets), the rest scrolls.
    self.notes = [[UITextView alloc] initWithFrame:CGRectMake(20, 100, 200, 100)];
    self.notes.tag = 1;
    self.notes.font = [UIFont systemFontOfSize:17];
    self.notes.delegate = self;
    [self.view addSubview:self.notes];

    self.readOnly = [[UITextView alloc] initWithFrame:CGRectMake(20, 220, 200, 60)];
    self.readOnly.tag = 2;
    self.readOnly.text = @"Read only";
    self.readOnly.editable = NO;
    [self.view addSubview:self.readOnly];

    self.sized = [[UITextView alloc] initWithFrame:CGRectMake(20, 300, 200, 60)];
    self.sized.tag = 3;
    self.sized.font = [UIFont systemFontOfSize:17];
    self.sized.text = @"One\nTwo";
    self.sized.textContainerInset = UIEdgeInsetsMake(4, 6, 4, 6);
    [self.view addSubview:self.sized];

    self.log = [self fieldAt:400 tag:100];
    self.scroll = [self fieldAt:440 tag:101];
    self.other = [self fieldAt:480 tag:102];
}

- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    CGSize fit = [self.sized sizeThatFits:CGSizeMake(200, CGFLOAT_MAX)];
    UIEdgeInsets inset = self.sized.textContainerInset;
    BOOL readOnly = [self.readOnly becomeFirstResponder];
    self.other.text = [NSString stringWithFormat:@"fit %.0fx%.0f inset %.0f,%.0f padding %.0f read-only %d", fit.width, fit.height, inset.top, inset.left,
                                                 self.sized.textContainer.lineFragmentPadding, readOnly];
    [self.notes becomeFirstResponder];
    [self update];
}

- (void)update {
    NSRange sel = self.notes.selectedRange;
    self.log.text = [NSString stringWithFormat:@"begin %d change %d selection %d end %d at %lu", self.begins, self.changes, self.selections, self.ends,
                                               (unsigned long)sel.location];
    self.scroll.text = [NSString stringWithFormat:@"content %.0f offset %.0f scrolls %d", self.notes.contentSize.height, self.notes.contentOffset.y,
                                                  self.scrolls];
}

- (void)textViewDidBeginEditing:(UITextView *)textView {
    self.begins++;
}

- (void)textViewDidEndEditing:(UITextView *)textView {
    self.ends++;
    [self update];
}

// No '#' allowed.
- (BOOL)textView:(UITextView *)textView shouldChangeTextInRange:(NSRange)range replacementText:(NSString *)text {
    return ![text isEqualToString:@"#"];
}

- (void)textViewDidChangeSelection:(UITextView *)textView {
    self.selections++;
}

- (void)textViewDidChange:(UITextView *)textView {
    self.changes++;
    [self update];
}

- (void)scrollViewDidScroll:(UIScrollView *)scrollView {
    self.scrolls++;
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
