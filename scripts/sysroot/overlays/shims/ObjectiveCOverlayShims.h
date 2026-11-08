// pomme: the autorelease-pool shims the ObjectiveC overlay uses.
#pragma once
extern void *objc_autoreleasePoolPush(void);
extern void objc_autoreleasePoolPop(void *pool);
static inline void *_swift_objc_autoreleasePoolPush(void) { return objc_autoreleasePoolPush(); }
static inline void _swift_objc_autoreleasePoolPop(void *pool) { objc_autoreleasePoolPop(pool); }
