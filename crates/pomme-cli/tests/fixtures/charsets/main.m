// NSCharacterSet and NSMutableCharacterSet, for tests/charsets.rs: each
// check is printed as name=0/1 in field 1.
#import <UIKit/UIKit.h>

@interface ViewController : UIViewController
@end

@implementation ViewController
- (void)viewDidLoad {
    [super viewDidLoad];
    // As Firebase checks library names: alphanumerics plus "-_.", inverted.
    NSMutableCharacterSet *allowed = [NSMutableCharacterSet alphanumericCharacterSet];
    [allowed addCharactersInString:@"-_."];
    NSCharacterSet *disallowed = [allowed invertedSet];
    NSCharacterSet *shared = [NSCharacterSet alphanumericCharacterSet];
    NSMutableCharacterSet *range = [NSMutableCharacterSet characterSetWithRange:NSMakeRange('a', 3)];
    [range removeCharactersInString:@"b"];
    [range addCharactersInRange:NSMakeRange('x', 2)];
    NSMutableCharacterSet *both = [[NSCharacterSet characterSetWithCharactersInString:@"ab"] mutableCopy];
    [both formIntersectionWithCharacterSet:[NSCharacterSet characterSetWithCharactersInString:@"bc"]];
    [both formUnionWithCharacterSet:[NSCharacterSet decimalDigitCharacterSet]];
    NSMutableCharacterSet *inverted = [NSMutableCharacterSet characterSetWithCharactersInString:@"q"];
    [inverted invert];
    NSArray *checks = @[
        @[@"added", @([allowed characterIsMember:'-'] && [allowed characterIsMember:'z'])],
        @[@"name", @([@"fire-core_1.0" rangeOfCharacterFromSet:disallowed].location == NSNotFound)],
        @[@"badName", @([@"fire core" rangeOfCharacterFromSet:disallowed].location == 4)],
        @[@"sharedUntouched", @(![shared characterIsMember:'-'])],
        @[@"range", @([range characterIsMember:'a'] && ![range characterIsMember:'b'] && [range characterIsMember:'c'] && [range characterIsMember:'y'])],
        @[@"intersect", @(![both characterIsMember:'a'] && [both characterIsMember:'b'] && ![both characterIsMember:'c'])],
        @[@"union", @([both characterIsMember:'7'])],
        @[@"invert", @(![inverted characterIsMember:'q'] && [inverted characterIsMember:'r'])],
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
