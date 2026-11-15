// The device's language and region, for tests/locale.rs: what NSLocale,
// CFLocale, the calendar, the time zone and the formatters report, and how
// they write a fixed date, numbers, prices and measurements. Each fact is a
// line `VALUE name text`. scripts/swiftui/metrics-simulator.sh records it on
// the Simulator once per locale (VARIANT=<locale>, with ARGS and TZ), as
// fixtures/locale/ios26-<locale>.txt.
#import <UIKit/UIKit.h>

static void value(NSString *name, id v) {
    NSString *s = [v isKindOfClass:NSArray.class] ? [v componentsJoinedByString:@","] : [NSString stringWithFormat:@"%@", v];
    // One fact per line: a newline in a value is written as \n.
    s = [s stringByReplacingOccurrencesOfString:@"\n" withString:@"\\n"];
    printf("VALUE %s %s\n", name.UTF8String, s.UTF8String);
    fflush(stdout);
}

static NSString *styleName(NSUInteger style) {
    return @[ @"none", @"short", @"medium", @"long", @"full" ][style];
}

static void report(void) {
    NSLocale *loc = NSLocale.currentLocale;
    NSUserDefaults *defaults = NSUserDefaults.standardUserDefaults;

    // The device's choice, as the app sees it in three places.
    value(@"defaults.AppleLanguages", [defaults objectForKey:@"AppleLanguages"] ?: @"nil");
    value(@"defaults.AppleLocale", [defaults objectForKey:@"AppleLocale"] ?: @"nil");
    value(@"locale.preferredLanguages", NSLocale.preferredLanguages);
    value(@"locale.identifier", loc.localeIdentifier);
    CFLocaleRef cf = CFLocaleCopyCurrent();
    value(@"cflocale.identifier", (__bridge NSString *)CFLocaleGetIdentifier(cf));
    CFRelease(cf);
    CFArrayRef cfLangs = CFLocaleCopyPreferredLanguages();
    value(@"cflocale.preferredLanguages", (__bridge NSArray *)cfLangs);
    CFRelease(cfLangs);

    // The locale's parts and region settings.
    value(@"locale.languageCode", loc.languageCode ?: @"nil");
    value(@"locale.countryCode", loc.countryCode ?: @"nil");
    value(@"locale.scriptCode", loc.scriptCode ?: @"nil");
    value(@"locale.calendarIdentifier", loc.calendarIdentifier);
    value(@"locale.usesMetricSystem", @(loc.usesMetricSystem));
    value(@"locale.measurementSystem", [loc objectForKey:NSLocaleMeasurementSystem]);
    value(@"locale.currencyCode", loc.currencyCode ?: @"nil");
    value(@"locale.currencySymbol", loc.currencySymbol);
    value(@"locale.decimalSeparator", loc.decimalSeparator);
    value(@"locale.groupingSeparator", loc.groupingSeparator);
    value(@"locale.quotationBeginDelimiter", loc.quotationBeginDelimiter);
    value(@"locale.quotationEndDelimiter", loc.quotationEndDelimiter);
    value(@"locale.alternateQuotationBeginDelimiter", loc.alternateQuotationBeginDelimiter);
    value(@"locale.alternateQuotationEndDelimiter", loc.alternateQuotationEndDelimiter);
    value(@"locale.characterDirection", @([NSLocale characterDirectionForLanguage:loc.languageCode]));
    value(@"locale.lineDirection", @([NSLocale lineDirectionForLanguage:loc.languageCode]));
    value(@"locale.displayName", [loc localizedStringForLocaleIdentifier:loc.localeIdentifier] ?: @"nil");
    value(@"locale.displayName.en_US", [[NSLocale localeWithLocaleIdentifier:@"en_US"] localizedStringForLocaleIdentifier:loc.localeIdentifier] ?: @"nil");
    value(@"locale.autoupdatingIsCurrent", @([NSLocale.autoupdatingCurrentLocale.localeIdentifier isEqual:loc.localeIdentifier]));

    // The calendar and the time zone.
    NSCalendar *cal = NSCalendar.currentCalendar;
    value(@"calendar.identifier", cal.calendarIdentifier);
    value(@"calendar.firstWeekday", @(cal.firstWeekday));
    value(@"calendar.minimumDaysInFirstWeek", @(cal.minimumDaysInFirstWeek));
    value(@"calendar.locale", cal.locale.localeIdentifier);
    value(@"calendar.shortWeekdaySymbols", cal.shortWeekdaySymbols);
    value(@"calendar.monthSymbols.0", cal.monthSymbols[0]);
    value(@"calendar.AMSymbol", cal.AMSymbol);
    value(@"timeZone.name", NSTimeZone.localTimeZone.name);

    // 12 or 24 hours: the hour field iOS picks for the locale.
    value(@"format.j", [NSDateFormatter dateFormatFromTemplate:@"j" options:0 locale:loc]);
    value(@"format.jmm", [NSDateFormatter dateFormatFromTemplate:@"jmm" options:0 locale:loc]);
    value(@"format.yMd", [NSDateFormatter dateFormatFromTemplate:@"yMd" options:0 locale:loc]);
    value(@"format.yMMMEd", [NSDateFormatter dateFormatFromTemplate:@"yMMMEd" options:0 locale:loc]);

    // A fixed instant, 2026-03-14 09:05:30 UTC, in the device's zone and locale.
    NSDate *date = [NSDate dateWithTimeIntervalSince1970:1773479130];
    NSDateFormatter *df = [NSDateFormatter new];
    for (NSUInteger s = NSDateFormatterShortStyle; s <= NSDateFormatterFullStyle; s++) {
        df.dateStyle = s;
        df.timeStyle = NSDateFormatterNoStyle;
        value([@"date." stringByAppendingString:styleName(s)], [df stringFromDate:date]);
        df.dateStyle = NSDateFormatterNoStyle;
        df.timeStyle = s;
        value([@"time." stringByAppendingString:styleName(s)], [df stringFromDate:date]);
        df.dateStyle = df.timeStyle = s;
        value([@"datetime." stringByAppendingString:styleName(s)], [df stringFromDate:date]);
    }
    df.dateStyle = df.timeStyle = NSDateFormatterNoStyle;
    df.dateFormat = @"yyyy-MM-dd HH:mm:ss zzz";
    value(@"date.fixed", [df stringFromDate:date]);
    df.dateFormat = nil;
    df.dateStyle = NSDateFormatterMediumStyle;
    df.doesRelativeDateFormatting = YES;
    value(@"date.relativeToday", [df stringFromDate:NSDate.date]);
    NSDateComponents *week = [cal components:NSCalendarUnitWeekOfYear | NSCalendarUnitWeekday fromDate:date];
    value(@"calendar.fixed.weekOfYear", @(week.weekOfYear));
    value(@"calendar.fixed.weekday", @(week.weekday));

    // Numbers.
    NSNumberFormatter *nf = [NSNumberFormatter new];
    NSDictionary *styles = @{
        @"decimal" : @(NSNumberFormatterDecimalStyle),
        @"currency" : @(NSNumberFormatterCurrencyStyle),
        @"percent" : @(NSNumberFormatterPercentStyle),
        @"scientific" : @(NSNumberFormatterScientificStyle),
        @"spellOut" : @(NSNumberFormatterSpellOutStyle),
        @"ordinal" : @(NSNumberFormatterOrdinalStyle),
        @"currencyISOCode" : @(NSNumberFormatterCurrencyISOCodeStyle),
        @"currencyPlural" : @(NSNumberFormatterCurrencyPluralStyle),
        @"currencyAccounting" : @(NSNumberFormatterCurrencyAccountingStyle),
    };
    for (NSString *name in [styles.allKeys sortedArrayUsingSelector:@selector(compare:)]) {
        nf.numberStyle = [styles[name] unsignedIntegerValue];
        value([@"number." stringByAppendingString:name], [nf stringFromNumber:@1234567.891]);
    }
    nf.numberStyle = NSNumberFormatterPercentStyle;
    value(@"number.percent.quarter", [nf stringFromNumber:@0.25]);
    nf.numberStyle = NSNumberFormatterOrdinalStyle;
    value(@"number.ordinal.3", [nf stringFromNumber:@3]);
    nf.numberStyle = NSNumberFormatterCurrencyStyle;
    value(@"price.1234.5", [nf stringFromNumber:@1234.5]);
    value(@"price.negative", [nf stringFromNumber:@-1234.5]);
    nf.numberStyle = NSNumberFormatterCurrencyAccountingStyle;
    value(@"price.negative.accounting", [nf stringFromNumber:@-1234.5]);
    value(@"number.localized", [NSNumberFormatter localizedStringFromNumber:@1234.5 numberStyle:NSNumberFormatterDecimalStyle]);

    // Measurements: the formatter converts to the region's units by default.
    NSMeasurementFormatter *mf = [NSMeasurementFormatter new];
    NSMeasurement *km = [[NSMeasurement alloc] initWithDoubleValue:5 unit:NSUnitLength.kilometers];
    NSMeasurement *c = [[NSMeasurement alloc] initWithDoubleValue:20 unit:NSUnitTemperature.celsius];
    NSMeasurement *kg = [[NSMeasurement alloc] initWithDoubleValue:2 unit:NSUnitMass.kilograms];
    value(@"measure.5km", [mf stringFromMeasurement:km]);
    value(@"measure.20C", [mf stringFromMeasurement:c]);
    value(@"measure.2kg", [mf stringFromMeasurement:kg]);
    mf.unitOptions = NSMeasurementFormatterUnitOptionsProvidedUnit;
    value(@"measure.5km.provided", [mf stringFromMeasurement:km]);

    // Lists, durations and comparing.
    value(@"list.three", [NSListFormatter localizedStringByJoiningStrings:@[ @"A", @"B", @"C" ]]);
    NSDateComponentsFormatter *dcf = [NSDateComponentsFormatter new];
    dcf.unitsStyle = NSDateComponentsFormatterUnitsStyleFull;
    value(@"duration.3725s", [dcf stringFromTimeInterval:3725]);
    NSByteCountFormatter *bcf = [NSByteCountFormatter new];
    value(@"bytes.1500000", [bcf stringFromByteCount:1500000]);
    value(@"string.uppercase.i", [@"i" uppercaseStringWithLocale:loc]);
    value(@"string.localizedUppercase.i", @"i".localizedUppercaseString);
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
