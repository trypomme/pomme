// Foundation collections compared by content, for tests/collections.rs: each
// check is printed as name=0/1 in field 1.
#import <UIKit/UIKit.h>

@interface ViewController : UIViewController
@end

@implementation ViewController
- (void)viewDidLoad {
    [super viewDidLoad];
    NSDictionary *nested1 = @{@"window": @{@"width": @(390.0), @"scale": @(3.0)}, @"fontScale": @(1)};
    NSDictionary *nested2 = @{@"window": @{@"width": [NSNumber numberWithDouble:390.0], @"scale": @(3.0)}, @"fontScale": @(1)};
    NSMutableDictionary *mutable = [nested1 mutableCopy];
    NSSet *s1 = [NSSet setWithObjects:@"a", @(2), nil], *s2 = [NSSet setWithArray:@[@(2), @"a"]];
    NSArray *checks = @[
        @[@"dict", @([nested1 isEqual:nested2])],
        @[@"dictHash", @(nested1.hash == nested2.hash)],
        @[@"mutable", @([mutable isEqual:nested1])],
        @[@"empty", @([@{} isEqual:[NSDictionary dictionary]])],
        @[@"differs", @([nested1 isEqual:@{@"fontScale": @(1)}])],
        @[@"notDict", @([nested1 isEqual:@[@1]])],
        @[@"set", @([s1 isEqual:s2])],
        @[@"setHash", @(s1.hash == s2.hash)],
        @[@"setDiffers", @([s1 isEqual:[NSSet setWithObject:@"a"]])],
        @[@"dictInSet", @([[NSSet setWithObject:nested1] containsObject:nested2])],
    ];
    NSMutableArray *out = [NSMutableArray new];
    for (NSArray *c in checks) [out addObject:[NSString stringWithFormat:@"%@=%@", c[0], c[1]]];
    UITextField *f = [[UITextField alloc] initWithFrame:CGRectMake(0, 100, 300, 30)];
    f.tag = 1;
    f.text = [out componentsJoinedByString:@" "];
    [self.view addSubview:f];
}
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)options {
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    self.window.rootViewController = [ViewController new];
    [self.window makeKeyAndVisible];
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
