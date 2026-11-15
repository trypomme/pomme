// Text in other scripts, for tests/intltext.rs: which fonts iOS falls back to
// (the system font's cascade list for each language, and the font that draws
// a character), and where TextKit breaks a Japanese paragraph into lines at a
// few widths. Each fact is a `VALUE name text` line; each line fragment a
// `METRIC name x y width height` line. scripts/swiftui/metrics-simulator.sh
// records it (VARIANT=en and VARIANT=ja, with ARGS for the languages).
#import <CoreText/CoreText.h>
#import <UIKit/UIKit.h>

static void value(NSString *name, NSString *v) {
    v = [v stringByReplacingOccurrencesOfString:@"\n" withString:@"\\n"];
    printf("VALUE %s %s\n", name.UTF8String, v.UTF8String);
    fflush(stdout);
}

static void metric(NSString *name, CGRect r) {
    printf("METRIC %s %.2f %.2f %.2f %.2f\n", name.UTF8String, r.origin.x, r.origin.y, r.size.width, r.size.height);
    fflush(stdout);
}

/// The Japanese paragraph: kanji, kana, small kana, brackets, punctuation and a Latin word.
static NSString *const kJapanese = @"日本語の文章は、単語の間に空白がなくても行の途中で折り返されます。小さい「ょ」や「っ」、句読点は行の頭に来ません。Pommeでも同じように折り返されるはずです。";

static UILabel *report(void) {
    UIFont *system = [UIFont systemFontOfSize:17];
    CTFontRef ct = (__bridge CTFontRef)system;

    // The cascade list: the fonts tried, in order, for characters the system font lacks.
    NSDictionary *languages = @{
        @"en" : @[ @"en" ], @"ja" : @[ @"ja" ], @"zh-Hans" : @[ @"zh-Hans" ], @"zh-Hant" : @[ @"zh-Hant" ],
        @"ko" : @[ @"ko" ], @"ar" : @[ @"ar" ], @"he" : @[ @"he" ], @"th" : @[ @"th" ],
    };
    for (NSString *key in [languages.allKeys sortedArrayUsingSelector:@selector(compare:)]) {
        NSArray *list = CFBridgingRelease(CTFontCopyDefaultCascadeListForLanguages(ct, (__bridge CFArrayRef)languages[key]));
        NSMutableArray *names = [NSMutableArray new];
        for (id d in list) {
            NSString *family = CFBridgingRelease(CTFontDescriptorCopyAttribute((__bridge CTFontDescriptorRef)d, kCTFontFamilyNameAttribute));
            [names addObject:family ?: @"?"];
        }
        value([@"cascade." stringByAppendingString:key], [names componentsJoinedByString:@","]);
    }

    // The font that draws each sample, with the device's own languages.
    NSDictionary *samples = @{
        @"hiragana" : @"あ", @"katakana" : @"カ", @"kanji" : @"漢", @"hangul" : @"한", @"arabic" : @"ا",
        @"hebrew" : @"א", @"thai" : @"ก", @"fullwidthComma" : @"、", @"ideographicStop" : @"。",
    };
    for (NSString *key in [samples.allKeys sortedArrayUsingSelector:@selector(compare:)]) {
        NSString *s = samples[key];
        CTFontRef f = CTFontCreateForString(ct, (__bridge CFStringRef)s, CFRangeMake(0, s.length));
        NSString *family = CFBridgingRelease(CTFontCopyFamilyName(f));
        value([@"font." stringByAppendingString:key], family);
        value([@"advance." stringByAppendingString:key], [NSString stringWithFormat:@"%.2f", [s sizeWithAttributes:@{NSFontAttributeName : system}].width]);
        CFRelease(f);
    }

    // The advance of every character in the paragraph, and of common Japanese
    // punctuation and samples of each kind (hiragana, katakana, kanji, full-width Latin).
    NSMutableOrderedSet *chars = [NSMutableOrderedSet new];
    NSString *more = @"「」『』（）［］【】〈〉《》〔〕・ー〜！？：；…‥、。，．々〃ゝゞヽヾぁぃぅぇぉゃゅょっゎァィゥェォャュョッヮヵヶあいうえおかきくけこアイウエオカキクケコ一二三四五六七八九十人大中小上下東京都ＡＢＣａｂｃ１２３　";
    // (Every character here is a single UTF-16 unit.)
    for (NSString *src in @[ kJapanese, more ]) {
        for (NSUInteger i = 0; i < src.length; i++) {
            [chars addObject:[src substringWithRange:NSMakeRange(i, 1)]];
        }
    }
    for (NSString *c in chars) {
        unichar u = [c characterAtIndex:0];
        value([NSString stringWithFormat:@"width.U+%04X", u], [NSString stringWithFormat:@"%.3f", [c sizeWithAttributes:@{NSFontAttributeName : system}].width]);
    }

    // Every pair of these: how much wider or narrower two characters are
    // together than apart (Japanese punctuation spacing), as `pair.U+XXXX.U+YYYY`.
    NSString *pairs = @"あ漢アa1「」『』（）【】〈〉《》〔〕［］、。，．・ー！？：";
    for (NSUInteger i = 0; i < pairs.length; i++) {
        for (NSUInteger j = 0; j < pairs.length; j++) {
            NSString *a = [pairs substringWithRange:NSMakeRange(i, 1)], *b = [pairs substringWithRange:NSMakeRange(j, 1)];
            CGFloat w = [[a stringByAppendingString:b] sizeWithAttributes:@{NSFontAttributeName : system}].width;
            CGFloat apart = [a sizeWithAttributes:@{NSFontAttributeName : system}].width + [b sizeWithAttributes:@{NSFontAttributeName : system}].width;
            if (fabs(w - apart) > 0.005) {
                value([NSString stringWithFormat:@"pair.U+%04X.U+%04X", [a characterAtIndex:0], [b characterAtIndex:0]], [NSString stringWithFormat:@"%.3f", w - apart]);
            }
        }
    }

    // The Japanese paragraph in TextKit at several widths: each line's characters and fragment.
    for (NSNumber *w in @[ @180, @200, @220, @250, @300 ]) {
        NSTextStorage *storage = [[NSTextStorage alloc] initWithString:kJapanese attributes:@{NSFontAttributeName : system}];
        NSLayoutManager *layout = [NSLayoutManager new];
        NSTextContainer *container = [[NSTextContainer alloc] initWithSize:CGSizeMake(w.doubleValue, CGFLOAT_MAX)];
        container.lineFragmentPadding = 0;
        [layout addTextContainer:container];
        [storage addLayoutManager:layout];
        __block int line = 0;
        [layout enumerateLineFragmentsForGlyphRange:NSMakeRange(0, layout.numberOfGlyphs)
                                         usingBlock:^(CGRect rect, CGRect used, NSTextContainer *c, NSRange glyphs, BOOL *stop) {
                                             NSRange chars = [layout characterRangeForGlyphRange:glyphs actualGlyphRange:NULL];
                                             NSString *name = [NSString stringWithFormat:@"ja.%d.line%d", w.intValue, line];
                                             value([name stringByAppendingString:@".text"], [kJapanese substringWithRange:chars]);
                                             metric(name, used);
                                             line++;
                                         }];
        value([NSString stringWithFormat:@"ja.%d.lines", w.intValue], [NSString stringWithFormat:@"%d", line]);
    }

    // A label, for the screenshot and its size.
    UILabel *label = [[UILabel alloc] initWithFrame:CGRectMake(20, 120, 200, 0)];
    label.numberOfLines = 0;
    label.font = system;
    label.text = kJapanese;
    label.textColor = UIColor.blackColor;
    [label sizeToFit];
    metric(@"label", label.frame);
    return label;
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
    [self.window addSubview:report()];
    return YES;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(AppDelegate.class));
    }
}
