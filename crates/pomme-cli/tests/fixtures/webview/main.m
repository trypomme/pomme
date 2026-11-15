// A web view app for tests/webkit.rs: a page it builds itself (red, posting a
// message, then calling home through a made-up app:// URL), a user script, a
// script message handler, a script whose result it reads back, and the
// page taking a tap and a drag. What
// happens is shown in a label (tag 100), one line each, sorted at the end.
#import <UIKit/UIKit.h>
#import <WebKit/WebKit.h>

@interface ViewController : UIViewController <WKNavigationDelegate, WKScriptMessageHandler>
@property(nonatomic, strong) WKWebView *web;
@property(nonatomic, strong) UILabel *events;
@property(nonatomic, strong) NSMutableArray<NSString *> *lines;
@end

@implementation ViewController
- (void)log:(NSString *)line {
    [self.lines addObject:line];
    self.events.text = [self.lines componentsJoinedByString:@"\n"];
}

- (NSString *)json:(id)value {
    if (!value) return @"nil";
    NSData *d = [NSJSONSerialization dataWithJSONObject:@[ value ] options:NSJSONWritingSortedKeys error:nil];
    NSString *s = [[NSString alloc] initWithData:d encoding:NSUTF8StringEncoding];
    return [s substringWithRange:NSMakeRange(1, s.length - 2)];
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;
    self.lines = [NSMutableArray new];
    WKWebViewConfiguration *config = [WKWebViewConfiguration new];
    [config.userContentController addScriptMessageHandler:self name:@"app"];
    WKUserScript *script = [[WKUserScript alloc] initWithSource:@"window.fromUserScript = 42;" injectionTime:WKUserScriptInjectionTimeAtDocumentStart forMainFrameOnly:YES];
    [config.userContentController addUserScript:script];
    self.web = [[WKWebView alloc] initWithFrame:CGRectMake(0, 100, 390, 200) configuration:config];
    self.web.navigationDelegate = self;
    [self.view addSubview:self.web];
    self.events = [[UILabel alloc] initWithFrame:CGRectMake(20, 340, 350, 300)];
    self.events.numberOfLines = 0;
    self.events.tag = 100;
    [self.view addSubview:self.events];
    // A tall page (it scrolls) with a button over its first screen (a tap presses it).
    // It says once that it scrolled down: the drag runs in virtual time and the
    // page in real time, so a wait for scrolling to stop would split it unevenly.
    NSString *html = @"<html><body style='margin:0;height:2000px;background:#ff0000'>"
                      "<button style='position:absolute;left:0;top:0;width:100%;height:200px;opacity:0' onclick=\"window.webkit.messageHandlers.app.postMessage('tapped')\"></button>"
                      "<script>var down = false; window.onscroll = function () { if (!down && window.scrollY > 50) { down = true; window.webkit.messageHandlers.app.postMessage('scrolled down'); } };</script>"
                      "<script>window.webkit.messageHandlers.app.postMessage({hello: 'pomme', n: 3});"
                      "setTimeout(function () { location.href = 'app://hello?from=' + location.host; }, 100);</script></body></html>";
    [self.web loadHTMLString:html baseURL:[NSURL URLWithString:@"https://example.com/"]];
}

- (void)webView:(WKWebView *)webView decidePolicyForNavigationAction:(WKNavigationAction *)action decisionHandler:(void (^)(WKNavigationActionPolicy))decisionHandler {
    NSURL *url = action.request.URL;
    if ([url.scheme isEqualToString:@"app"]) {
        [self log:[NSString stringWithFormat:@"called home: %@", url.absoluteString]];
        decisionHandler(WKNavigationActionPolicyCancel);
        [webView evaluateJavaScript:@"({sum: 1 + 2, list: [true, 'x'], fromUserScript: window.fromUserScript, title: document.title})" completionHandler:^(id result, NSError *error) {
            [self log:[NSString stringWithFormat:@"result %@", error ? error.localizedDescription : [self json:result]]];
            [webView evaluateJavaScript:@"undefined" completionHandler:^(id nothing, NSError *e) {
                [self log:[NSString stringWithFormat:@"undefined is %@", nothing ? @"something" : @"nil"]];
            }];
        }];
        return;
    }
    [self log:[NSString stringWithFormat:@"allow %@", url.absoluteString]];
    decisionHandler(WKNavigationActionPolicyAllow);
}

- (void)webView:(WKWebView *)webView didFinishNavigation:(WKNavigation *)navigation {
    [self log:[NSString stringWithFormat:@"finished %@", webView.URL.absoluteString]];
}

- (void)userContentController:(WKUserContentController *)controller didReceiveScriptMessage:(WKScriptMessage *)message {
    [self log:[NSString stringWithFormat:@"message %@: %@", message.name, [self json:message.body]]];
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
