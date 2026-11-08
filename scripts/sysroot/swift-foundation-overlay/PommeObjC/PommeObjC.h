// Declarations of the Foundation Objective-C classes that Swift Foundation's
// public API mentions. pomme implements them (HLE); this header only lets the
// overlay name them. Imported Objective-C classes mangle as `So<name>C` no
// matter which module declares them, so the overlay's symbols match Apple's.
#import <objc/NSObject.h>
#include <CoreFoundation/CFAvailability.h>
#include <stdarg.h>

// Apple's spellings, so imported types mangle as Apple's do (`So20NSDataReadingOptionsV`, ...).
#define NS_ENUM(_type, _name) CF_ENUM(_type, _name)
#define NS_OPTIONS(_type, _name) CF_OPTIONS(_type, _name)
#define NS_TYPED_EXTENSIBLE_ENUM __attribute__((swift_wrapper(struct)))

typedef struct {
    unsigned long state;
    id __unsafe_unretained _Nullable * _Nullable itemsPtr;
    unsigned long * _Nullable mutationsPtr;
    unsigned long extra[5];
} NSFastEnumerationState;

@protocol NSFastEnumeration
- (unsigned long)countByEnumeratingWithState:(NSFastEnumerationState *)state objects:(id __unsafe_unretained _Nullable [_Nonnull])buffer count:(unsigned long)len;
@end

@interface NSString : NSObject
@property (readonly) unsigned long length;
- (instancetype)initWithFormat:(NSString *)format arguments:(va_list)argList;
@end

typedef NS_OPTIONS(unsigned long, NSStringCompareOptions) {
    NSCaseInsensitiveSearch = 1,
    NSLiteralSearch = 2,
    NSBackwardsSearch = 4,
    NSAnchoredSearch = 8,
    NSNumericSearch = 64,
    NSDiacriticInsensitiveSearch = 128,
    NSWidthInsensitiveSearch = 256,
    NSForcedOrderingSearch = 512,
    NSRegularExpressionSearch = 1024,
};

@interface NSArray : NSObject <NSFastEnumeration>
@property (readonly) unsigned long count;
- (id)objectAtIndex:(unsigned long)index;
@end

@interface NSDictionary : NSObject <NSFastEnumeration>
@property (readonly) unsigned long count;
@property (readonly) NSArray *allKeys;
- (id)objectForKey:(id)key;
@end

@interface NSSet : NSObject <NSFastEnumeration>
@property (readonly) unsigned long count;
@property (readonly) NSArray *allObjects;
@end

@interface NSNumber : NSObject
- (instancetype)initWithBool:(BOOL)value;
- (instancetype)initWithLongLong:(long long)value;
- (instancetype)initWithUnsignedLongLong:(unsigned long long)value;
- (instancetype)initWithDouble:(double)value;
- (instancetype)initWithFloat:(float)value;
@property (readonly) BOOL boolValue;
@property (readonly) long long longLongValue;
@property (readonly) unsigned long long unsignedLongLongValue;
@property (readonly) double doubleValue;
@property (readonly) float floatValue;
@end

@interface NSURL : NSObject
- (nullable instancetype)initWithString:(NSString *)string;
@property (readonly) NSString *absoluteString;
@end

#pragma clang assume_nonnull begin

typedef NS_OPTIONS(unsigned long, NSDataReadingOptions) {
    NSDataReadingMappedIfSafe = 1UL << 0,
    NSDataReadingUncached = 1UL << 1,
    NSDataReadingMappedAlways = 1UL << 3,
};
typedef NS_OPTIONS(unsigned long, NSDataWritingOptions) {
    NSDataWritingAtomic = 1UL << 0,
    NSDataWritingWithoutOverwriting = 1UL << 1,
};
typedef NS_OPTIONS(unsigned long, NSDataBase64EncodingOptions) {
    NSDataBase64Encoding64CharacterLineLength = 1UL << 0,
    NSDataBase64Encoding76CharacterLineLength = 1UL << 1,
    NSDataBase64EncodingEndLineWithCarriageReturn = 1UL << 4,
    NSDataBase64EncodingEndLineWithLineFeed = 1UL << 5,
};
typedef NS_OPTIONS(unsigned long, NSDataBase64DecodingOptions) {
    NSDataBase64DecodingIgnoreUnknownCharacters = 1UL << 0,
};

@interface NSData : NSObject
- (instancetype)initWithBytes:(nullable const void *)bytes length:(unsigned long)length;
@property (nullable, readonly) const void *bytes;
@property (readonly) unsigned long length;
@end

@interface NSDate : NSObject
- (instancetype)initWithTimeIntervalSinceReferenceDate:(double)ti;
@property (readonly) double timeIntervalSinceReferenceDate;
@end

@interface NSLocale : NSObject
- (instancetype)initWithLocaleIdentifier:(NSString *)string;
@end

typedef NSString *NSNotificationName NS_TYPED_EXTENSIBLE_ENUM;
typedef NSString *NSURLResourceKey NS_TYPED_EXTENSIBLE_ENUM;

@interface NSNotification : NSObject
@property (readonly, copy) NSNotificationName name;
@property (nullable, readonly, retain) id object;
@property (nullable, readonly, copy) NSDictionary *userInfo;
@end

@interface NSError : NSObject
- (instancetype)initWithDomain:(NSString *)domain code:(long)code userInfo:(nullable NSDictionary *)dict;
@property (readonly, copy) NSString *domain;
@property (readonly) long code;
@property (readonly, copy) NSDictionary *userInfo;
@property (readonly, copy) NSString *localizedDescription;
@end

@interface NSInputStream : NSObject
@end

/// CoreFoundation's error type, toll-free bridged to NSError (imports as `CFError`).
typedef struct __attribute__((objc_bridge(NSError))) __CFError *CFErrorRef;

typedef NS_ENUM(unsigned long, NSURLRequestCachePolicy) {
    NSURLRequestUseProtocolCachePolicy = 0,
    NSURLRequestReloadIgnoringLocalCacheData = 1,
    NSURLRequestReloadIgnoringLocalAndRemoteCacheData = 4,
    NSURLRequestReturnCacheDataElseLoad = 2,
    NSURLRequestReturnCacheDataDontLoad = 3,
    NSURLRequestReloadRevalidatingCacheData = 5,
};

@interface NSURLRequest : NSObject
- (instancetype)initWithURL:(NSURL *)URL cachePolicy:(NSURLRequestCachePolicy)cachePolicy timeoutInterval:(double)timeoutInterval;
@property (nullable, readonly, copy) NSURL *URL;
@property (readonly) NSURLRequestCachePolicy cachePolicy;
@property (readonly) double timeoutInterval;
@property (nullable, readonly, copy) NSString *HTTPMethod;
@property (nullable, readonly, copy) NSDictionary *allHTTPHeaderFields;
@property (nullable, readonly, copy) NSData *HTTPBody;
@property (nullable, readonly, retain) NSInputStream *HTTPBodyStream;
@end

@interface NSMutableURLRequest : NSURLRequest
- (void)setHTTPMethod:(NSString *)method;
- (void)setHTTPBody:(nullable NSData *)body;
- (void)setValue:(nullable NSString *)value forHTTPHeaderField:(NSString *)field;
@end



#pragma clang assume_nonnull end
