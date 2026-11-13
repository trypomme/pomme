// An app that plays a YouTube video with Google's YouTube iOS player helper
// (YTPlayerView.h/.m and resources/YTPlayerView-iframe-player.html, copied
// unchanged from github.com/youtube/youtube-ios-player-helper), for
// tests/webkit.rs. What the player tells its delegate is shown in a label
// (tag 100), one event per line; the view dump prints it.
#import <UIKit/UIKit.h>
#import "YTPlayerView.h"

static NSString *stateName(YTPlayerState s) {
    switch (s) {
        case kYTPlayerStateUnstarted: return @"unstarted";
        case kYTPlayerStateEnded: return @"ended";
        case kYTPlayerStatePlaying: return @"playing";
        case kYTPlayerStatePaused: return @"paused";
        case kYTPlayerStateBuffering: return @"buffering";
        case kYTPlayerStateCued: return @"cued";
        default: return @"unknown";
    }
}

@interface ViewController : UIViewController <YTPlayerViewDelegate>
@property(nonatomic, strong) YTPlayerView *player;
@property(nonatomic, strong) UILabel *events;
@end

@implementation ViewController
- (void)log:(NSString *)event {
    self.events.text = self.events.text.length ? [self.events.text stringByAppendingFormat:@"\n%@", event] : event;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;
    self.player = [[YTPlayerView alloc] initWithFrame:CGRectMake(0, 100, 390, 220)];
    self.player.delegate = self;
    [self.view addSubview:self.player];
    self.events = [[UILabel alloc] initWithFrame:CGRectMake(20, 340, 350, 300)];
    self.events.numberOfLines = 0;
    self.events.tag = 100;
    [self.view addSubview:self.events];
    BOOL loaded = [self.player loadWithVideoId:@"M7lc1UVf-VE" playerVars:@{@"playsinline": @1}];
    [self log:loaded ? @"loading" : @"load failed"];
}

- (void)playerViewDidBecomeReady:(YTPlayerView *)playerView {
    [self log:@"ready"];
    [playerView duration:^(double duration, NSError *error) {
        [self log:error ? [NSString stringWithFormat:@"duration error %ld", (long)error.code] : [NSString stringWithFormat:@"duration %.0f", duration]];
    }];
    [playerView playVideo];
}

- (void)playerView:(YTPlayerView *)playerView didChangeToState:(YTPlayerState)state {
    [self log:[NSString stringWithFormat:@"state %@", stateName(state)]];
    if (state == kYTPlayerStatePlaying) {
        [playerView playerState:^(YTPlayerState current, NSError *error) {
            [self log:[NSString stringWithFormat:@"asked: %@", stateName(current)]];
        }];
    }
}

- (void)playerView:(YTPlayerView *)playerView receivedError:(YTPlayerError)error {
    [self log:[NSString stringWithFormat:@"error %ld", (long)error]];
}
@end

@interface AppDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation AppDelegate
- (BOOL)application:(UIApplication *)application didFinishLaunchingWithOptions:(NSDictionary *)launchOptions {
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
