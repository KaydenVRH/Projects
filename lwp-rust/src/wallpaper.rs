use std::ptr::NonNull;
use std::sync::atomic::Ordering;

use objc2::MainThreadOnly;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSApplication, NSApplicationActivationPolicy, NSBackingStoreType,
    NSColor, NSScreen, NSView, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_av_foundation::{
    AVLayerVideoGravityResizeAspectFill, AVPlayerItem, AVPlayerLayer, AVQueuePlayer,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSDate, NSRect, NSRunLoop, NSString, NSURL};

use crate::SHOULD_QUIT;

const WINDOW_LEVEL: isize = -10000;

struct ScreenWindow {
    window: Retained<NSWindow>,
    player: Retained<AVQueuePlayer>,
}

unsafe fn create_window(mtm: MainThreadMarker, frame: NSRect, url: &NSURL) -> ScreenWindow {
    let win: Retained<NSWindow> = msg_send![
        NSWindow::alloc(mtm),
        initWithContentRect: frame,
        styleMask: NSWindowStyleMask::Borderless,
        backing: NSBackingStoreType::Buffered,
        defer: false,
    ];
    win.setTitle(&NSString::from_str("Live Wallpaper"));
    win.setOpaque(true);
    win.setBackgroundColor(Some(&NSColor::blackColor()));
    win.setLevel(WINDOW_LEVEL);
    win.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    win.setIgnoresMouseEvents(true);

    let item_a = AVPlayerItem::initWithURL(AVPlayerItem::alloc(mtm), url);
    let item_b = AVPlayerItem::initWithURL(AVPlayerItem::alloc(mtm), url);

    let mut objs = [
        NonNull::new(&*item_a as *const _ as *mut _).unwrap(),
        NonNull::new(&*item_b as *const _ as *mut _).unwrap(),
    ];
    let items =
        NSArray::<AVPlayerItem>::arrayWithObjects_count(NonNull::new(objs.as_mut_ptr()).unwrap(), 2);
    let queue_player = AVQueuePlayer::queuePlayerWithItems(&items, mtm);
    queue_player.setVolume(0.0);

    let view: Retained<NSView> = msg_send![NSView::alloc(mtm), initWithFrame: frame];
    view.setWantsLayer(true);
    view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );

    let player_layer = AVPlayerLayer::playerLayerWithPlayer(Some(&queue_player));
    player_layer.setVideoGravity(AVLayerVideoGravityResizeAspectFill.unwrap());
    {
        let bounds = view.bounds();
        let _: () = msg_send![&player_layer, setFrame: bounds];
        let _: () = msg_send![&player_layer, setAutoresizingMask: 2u32 | 16u32];
    }

    if let Some(layer) = view.layer() {
        let _: () = msg_send![&*layer, addSublayer: &*player_layer];
    }

    win.setContentView(Some(&view));
    win.makeKeyAndOrderFront(None);
    queue_player.play();

    ScreenWindow {
        window: win,
        player: queue_player,
    }
}

fn tear_down(windows: &mut Vec<ScreenWindow>) {
    for sw in windows.drain(..) {
        unsafe {
            sw.player.pause();
            sw.window.setReleasedWhenClosed(false);
            sw.window.orderOut(None);
            sw.window.close();
        }
    }
}

fn build_all(mtm: MainThreadMarker, screens: &NSArray<NSScreen>, url: &NSURL) -> Vec<ScreenWindow> {
    screens
        .iter()
        .map(|screen| unsafe { create_window(mtm, screen.frame(), url) })
        .collect()
}

fn frames_of(screens: &NSArray<NSScreen>) -> Vec<[f64; 4]> {
    let mut frames: Vec<[f64; 4]> = screens
        .iter()
        .map(|screen| {
            let f = screen.frame();
            [f.origin.x, f.origin.y, f.size.width, f.size.height]
        })
        .collect();
    frames.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    frames
}

pub fn run(path: &str, _hide_icons: bool) {
    unsafe {
        libc::signal(libc::SIGTERM, sigterm_handler as *const () as usize);
    }

    let mtm = MainThreadMarker::new().expect("must be on main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app.finishLaunching();

    let url = NSURL::fileURLWithPath(&NSString::from_str(path));

    let screens = NSScreen::screens(mtm);
    let mut last_frames = frames_of(&screens);
    let mut windows = build_all(mtm, &screens, &url);

    let run_loop = NSRunLoop::currentRunLoop();
    loop {
        unsafe {
            if SHOULD_QUIT.load(Ordering::SeqCst) {
                break;
            }

            let screens = NSScreen::screens(mtm);
            let current_frames = frames_of(&screens);
            if current_frames != last_frames {
                tear_down(&mut windows);
                windows = build_all(mtm, &screens, &url);
                last_frames = current_frames;
            }

            for sw in &windows {
                let queue_items = sw.player.items();
                if queue_items.len() < 2 {
                    let after = queue_items.lastObject();
                    let new_item = AVPlayerItem::initWithURL(AVPlayerItem::alloc(mtm), &url);
                    sw.player.insertItem_afterItem(&new_item, after.as_deref());
                }
            }

            let limit = NSDate::dateWithTimeIntervalSinceNow(0.1);
            run_loop.runUntilDate(&limit);
        }
    }

    tear_down(&mut windows);
}

extern "C" fn sigterm_handler(_: i32) {
    SHOULD_QUIT.store(true, Ordering::SeqCst);
}
