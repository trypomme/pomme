// The app's own translations, for tests/localized.rs: which localization the
// bundle picks, the strings, plurals and Info.plist values it reads from its
// .lproj folders (resources/), and where it finds a resource. Each fact is a
// line `VALUE name text`. scripts/swiftui/metrics-simulator.sh records it on
// the Simulator once per language (VARIANT=<locale>, with ARGS), as
// fixtures/localized/ios26-<locale>.txt.
#import <UIKit/UIKit.h>

static void value(NSString *name, id v) {
    NSString *s = [v isKindOfClass:NSArray.class] ? [v componentsJoinedByString:@","] : [NSString stringWithFormat:@"%@", v ?: @"nil"];
    s = [s stringByReplacingOccurrencesOfString:@"\n" withString:@"\\n"];
    printf("VALUE %s %s\n", name.UTF8String, s.UTF8String);
    fflush(stdout);
}

/// A resource's path from the bundle, or "nil".
static NSString *where(NSString *path) {
    if (!path) return @"nil";
    NSString *root = NSBundle.mainBundle.bundlePath;
    return [path hasPrefix:root] ? [path substringFromIndex:root.length + 1] : path;
}

static void report(void) {
    NSBundle *b = NSBundle.mainBundle;
    value(@"bundle.localizations", [b.localizations sortedArrayUsingSelector:@selector(compare:)]);
    value(@"bundle.preferredLocalizations", b.preferredLocalizations);
    value(@"bundle.developmentLocalization", b.developmentLocalization);
    value(@"locale.identifier", NSLocale.currentLocale.localeIdentifier);

    // How CoreFoundation matches preferences with a bundle's localizations.
    NSDictionary *sets = @{
        @"A" : @[ @"en", @"ja" ],
        @"B" : @[ @"Base", @"en", @"en-GB", @"en-AU", @"ja", @"zh-Hans", @"zh-Hant", @"zh-HK", @"pt", @"pt-PT", @"pt-BR", @"es", @"es-419", @"fr", @"fr-CA" ],
        @"C" : @[ @"English", @"Japanese", @"French" ],
        @"D" : @[ @"de" ],
        @"E" : @[ @"en-US", @"ja-JP", @"zh_CN", @"zh_TW" ],
    };
    NSArray *prefs = @[
        @[ @"ja-JP" ], @[ @"en-AU" ], @[ @"en-GB" ], @[ @"en-IN" ], @[ @"en" ], @[ @"fr", @"ja" ], @[ @"fr-CA" ], @[ @"de" ],
        @[ @"zh-Hant-HK" ], @[ @"zh-Hant-TW" ], @[ @"zh-Hans-CN" ], @[ @"zh-HK" ], @[ @"pt-BR" ], @[ @"pt-PT" ], @[ @"pt" ],
        @[ @"es-MX" ], @[ @"es-ES" ], @[ @"ja-JP", @"en-US" ], @[ @"de", @"fr" ], @[ @"en-US" ], @[ @"zh-Hans" ], @[ @"zh-TW" ],
    ];
    for (NSString *set in [sets.allKeys sortedArrayUsingSelector:@selector(compare:)]) {
        for (NSArray *p in prefs) {
            NSArray *got = [NSBundle preferredLocalizationsFromArray:sets[set] forPreferences:p];
            value([NSString stringWithFormat:@"pref.%@.%@", set, [p componentsJoinedByString:@"|"]], got);
        }
        value([NSString stringWithFormat:@"pref.%@.current", set], [NSBundle preferredLocalizationsFromArray:sets[set]]);
    }

    // Strings.
    value(@"string.greeting", NSLocalizedString(@"greeting", nil));
    value(@"string.only.en", NSLocalizedString(@"only.en", nil));
    value(@"string.escaped", NSLocalizedString(@"escaped", nil));
    value(@"string.missing", NSLocalizedString(@"missing.key", nil));
    value(@"string.missing.value", [b localizedStringForKey:@"missing.key" value:@"Fallback" table:nil]);
    value(@"string.missing.empty", [b localizedStringForKey:@"missing.key" value:@"" table:nil]);
    value(@"string.greeting.value", [b localizedStringForKey:@"greeting" value:@"Fallback" table:nil]);
    value(@"string.other", NSLocalizedStringFromTable(@"farewell", @"Other", nil));
    value(@"string.nosuchtable", NSLocalizedStringFromTable(@"farewell", @"NoSuchTable", nil));
    value(@"string.items.raw", NSLocalizedString(@"items", nil));

    // Plurals (.stringsdict).
    for (NSNumber *n in @[ @0, @1, @2, @5 ]) {
        value([NSString stringWithFormat:@"plural.items.%@", n], [NSString localizedStringWithFormat:NSLocalizedString(@"items", nil), n.intValue]);
        value([NSString stringWithFormat:@"plural.format.%@", n], [NSString stringWithFormat:NSLocalizedString(@"items", nil), n.intValue]);
        value([NSString stringWithFormat:@"plural.files.%@", n], [NSString localizedStringWithFormat:NSLocalizedString(@"files", nil), (long)n.integerValue, @"Docs"]);
    }

    // Info.plist values (InfoPlist.strings).
    value(@"info.displayName", [b objectForInfoDictionaryKey:@"CFBundleDisplayName"]);
    value(@"info.camera", [b objectForInfoDictionaryKey:@"NSCameraUsageDescription"]);
    value(@"info.localized.displayName", b.localizedInfoDictionary[@"CFBundleDisplayName"]);
    value(@"info.localized.camera", b.localizedInfoDictionary[@"NSCameraUsageDescription"]);
    value(@"info.localized.identifier", b.localizedInfoDictionary[@"CFBundleIdentifier"]);
    value(@"info.plain.displayName", b.infoDictionary[@"CFBundleDisplayName"]);

    // Resources.
    for (NSString *name in @[ @"note", @"base", @"top", @"Localizable", @"both" ]) {
        NSString *ext = [name isEqual:@"Localizable"] ? @"strings" : @"txt";
        value([@"resource." stringByAppendingString:name], where([b pathForResource:name ofType:ext]));
        value([@"url." stringByAppendingString:name], where([b URLForResource:name withExtension:ext].path));
    }
    value(@"resource.note.ja", where([b pathForResource:@"note" ofType:@"txt" inDirectory:nil forLocalization:@"ja"]));
    value(@"resource.note.en", where([b pathForResource:@"note" ofType:@"txt" inDirectory:nil forLocalization:@"en"]));
    value(@"resources.txt", [[[b pathsForResourcesOfType:@"txt" inDirectory:nil] valueForKey:@"lastPathComponent"] sortedArrayUsingSelector:@selector(compare:)]);
    // In Base.lproj and in each language's folder.
    value(@"resource.both", where([b pathForResource:@"both" ofType:@"txt"]));
    NSString *note = [b pathForResource:@"note" ofType:@"txt"];
    value(@"resource.note.text", note ? [[NSString stringWithContentsOfFile:note encoding:NSUTF8StringEncoding error:nil] stringByTrimmingCharactersInSet:NSCharacterSet.newlineCharacterSet] : @"nil");
}

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)app didFinishLaunchingWithOptions:(NSDictionary *)options {
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    self.window.rootViewController = [UIViewController new];
    self.window.rootViewController.view.backgroundColor = UIColor.whiteColor;
    [self.window makeKeyAndVisible];
    report();
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
