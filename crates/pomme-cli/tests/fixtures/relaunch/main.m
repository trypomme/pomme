// Relaunching (tests/relaunch.rs): at each launch the app prints what it
// finds of the last one, then saves its own: a default (a launch count), a
// file in Documents, a keychain item, and a global variable (which no launch
// keeps). On the Simulator: run it, quit it, run it again.
#import <UIKit/UIKit.h>
#import <Security/Security.h>

static int launchesInMemory = 0;

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)options {
    NSUserDefaults *d = NSUserDefaults.standardUserDefaults;
    NSInteger launches = [d integerForKey:@"launches"] + 1;
    [d setInteger:launches forKey:@"launches"];
    // (So a quit right after can't lose it.)
    [d synchronize];
    printf("VALUE launch %ld\n", (long)launches);
    launchesInMemory += 1;
    printf("VALUE inMemory %d\n", launchesInMemory);

    NSString *docs = NSSearchPathForDirectoriesInDomains(NSDocumentDirectory, NSUserDomainMask, YES)[0];
    NSString *path = [docs stringByAppendingPathComponent:@"note.txt"];
    NSString *note = [NSString stringWithContentsOfFile:path encoding:NSUTF8StringEncoding error:nil];
    printf("VALUE file %s\n", note ? note.UTF8String : "(none)");
    [[NSString stringWithFormat:@"written at launch %ld", (long)launches] writeToFile:path atomically:YES encoding:NSUTF8StringEncoding error:nil];

    NSString *library = NSSearchPathForDirectoriesInDomains(NSLibraryDirectory, NSUserDomainMask, YES)[0];
    NSString *prefs = [library stringByAppendingFormat:@"/Preferences/%@.plist", NSBundle.mainBundle.bundleIdentifier];
    NSDictionary *saved = [NSDictionary dictionaryWithContentsOfFile:prefs];
    printf("VALUE prefsFile %s\n", saved ? [[saved[@"launches"] description] UTF8String] : "(none)");

    NSDictionary *query = @{ (id)kSecClass: (id)kSecClassGenericPassword, (id)kSecAttrService: @"pomme", (id)kSecAttrAccount: @"probe", (id)kSecReturnData: @YES };
    CFTypeRef found = NULL;
    if (SecItemCopyMatching((CFDictionaryRef)query, &found) == errSecSuccess) {
        NSString *s = [[NSString alloc] initWithData:(__bridge_transfer NSData *)found encoding:NSUTF8StringEncoding];
        printf("VALUE keychain %s\n", s.UTF8String);
    } else {
        printf("VALUE keychain (none)\n");
        NSDictionary *item = @{ (id)kSecClass: (id)kSecClassGenericPassword, (id)kSecAttrService: @"pomme", (id)kSecAttrAccount: @"probe", (id)kSecValueData: [@"secret" dataUsingEncoding:NSUTF8StringEncoding] };
        OSStatus added = SecItemAdd((CFDictionaryRef)item, NULL);
        printf("VALUE keychainAdd %d\n", (int)added);
    }
    fflush(stdout);

    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    UIViewController *root = [UIViewController new];
    root.view.backgroundColor = UIColor.whiteColor;
    self.window.rootViewController = root;
    [self.window makeKeyAndVisible];
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
