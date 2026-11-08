// Declarations of the Foundation Objective-C classes that Swift Foundation's
// public API mentions. pomme implements them (HLE); this header only lets the
// overlay name them. Imported Objective-C classes mangle as `So<name>C` no
// matter which module declares them, so the overlay's symbols match Apple's.
#import <objc/NSObject.h>

@interface NSString : NSObject
@property (readonly) unsigned long length;
@end

@interface NSArray : NSObject
@property (readonly) unsigned long count;
- (id)objectAtIndex:(unsigned long)index;
@end

@interface NSDictionary : NSObject
@property (readonly) unsigned long count;
@property (readonly) NSArray *allKeys;
- (id)objectForKey:(id)key;
@end

@interface NSSet : NSObject
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
@end

@interface NSData : NSObject
@end

@interface NSDate : NSObject
@end

@interface NSLocale : NSObject
@end

@interface NSNotification : NSObject
@end

@interface NSError : NSObject
@end
