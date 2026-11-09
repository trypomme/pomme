// A small app that throws and catches C++ exceptions, for tests/exceptions.rs.
// Each result is shown in a text field, told apart by its tag.
#import <UIKit/UIKit.h>
#include <stdexcept>
#include <string>

static std::string trail;

// Leaves a mark when it's destroyed: unwinding must run destructors.
struct Mark {
    const char *name;
    ~Mark() { trail += name; }
};

[[gnu::noinline]] static int deep(int n) {
    Mark m{"d"};
    if (n == 0) {
        throw std::runtime_error("from deep down");
    }
    return deep(n - 1) + 1;
}

[[gnu::noinline]] static std::string rethrown() {
    try {
        try {
            throw std::logic_error("first");
        } catch (const std::exception &e) {
            Mark m{"r"};
            throw;  // rethrow the same exception
        }
    } catch (const std::logic_error &e) {
        return std::string("rethrown ") + e.what();
    }
    return "not caught";
}

[[gnu::noinline]] static std::string by_type() {
    try {
        throw 42;
    } catch (const std::exception &) {
        return "wrong handler";
    } catch (int v) {
        return "int " + std::to_string(v);
    }
}

@interface ViewController : UIViewController
@end

@implementation ViewController
- (void)show:(std::string)text tag:(NSInteger)tag {
    UITextField *f = [[UITextField alloc] initWithFrame:CGRectMake(20, 80 + 50 * (tag - 100), 350, 40)];
    f.tag = tag;
    f.text = [NSString stringWithUTF8String:text.c_str()];
    [self.view addSubview:f];
}

- (void)viewDidLoad {
    [super viewDidLoad];
    // 100: caught several frames up, every frame's destructor run on the way.
    std::string caught;
    try {
        deep(3);
        caught = "not thrown";
    } catch (const std::runtime_error &e) {
        caught = std::string("caught ") + e.what() + " after " + trail;
    }
    [self show:caught tag:100];
    // 101: a handler that rethrows, caught by an outer one.
    trail.clear();
    std::string r = rethrown();
    [self show:r + " after " + trail tag:101];
    // 102: handlers are matched by type.
    [self show:by_type() tag:102];
    // 103: still running normally afterwards.
    [self show:"done" tag:103];
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
