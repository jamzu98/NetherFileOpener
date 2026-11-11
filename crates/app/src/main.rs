//! Nether File Opener — a tiny NFO viewer.

use std::cell::{Cell, OnceCell, RefCell};
use std::path::Path;
use std::ptr::NonNull;

use nfo_core::{DEFAULT_SIZE, PREF_LIGHT, PREF_SIZE, SIZES};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{define_class, msg_send, sel, AnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAboutPanelOptionCredits, NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSAutoresizingMaskOptions,
    NSBackingStoreType, NSColor, NSEventModifierFlags, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSLinkAttributeName, NSMenu, NSMenuItem, NSModalResponseOK,
    NSMutableParagraphStyle, NSOpenPanel, NSParagraphStyleAttributeName, NSScrollView,
    NSTextAlignment, NSTextView, NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_core_foundation::{CFData, CFRetained, CGFloat};
use objc2_core_text::{CTFont, CTFontDescriptor, CTFontManagerCreateFontDescriptorFromData};
use objc2_foundation::{
    ns_string, NSArray, NSDictionary, NSMutableAttributedString, NSNotification, NSObject,
    NSObjectProtocol, NSPoint, NSRange, NSRect, NSSize, NSString, NSURL, NSUserDefaults,
};

const INSET: CGFloat = 16.0;
const MIN_COLS: usize = 80;

struct Doc {
    window: Retained<NSWindow>,
    text: Retained<NSTextView>,
    content: String,
}

thread_local! {
    static DOCS: RefCell<Vec<Doc>> = const { RefCell::new(Vec::new()) };
    static FONT_DESC: OnceCell<CFRetained<CTFontDescriptor>> = const { OnceCell::new() };
    static CASCADE: Cell<NSPoint> = const { Cell::new(NSPoint::new(0.0, 0.0)) };
}

// ---------- prefs ----------

fn defaults() -> Retained<NSUserDefaults> {
    NSUserDefaults::standardUserDefaults()
}

fn light() -> bool {
    defaults().boolForKey(&NSString::from_str(PREF_LIGHT))
}

fn size() -> f64 {
    let s = defaults().doubleForKey(&NSString::from_str(PREF_SIZE));
    if SIZES.contains(&s) { s } else { DEFAULT_SIZE }
}

// ---------- rendering ----------

fn font(size: f64) -> Retained<NSFont> {
    FONT_DESC.with(|cell| {
        let desc = cell.get_or_init(|| {
            let data = CFData::from_bytes(nfo_core::FONT_TTF);
            unsafe { CTFontManagerCreateFontDescriptorFromData(&data) }.expect("bundled font")
        });
        let ct = unsafe { CTFont::with_font_descriptor(desc, size, std::ptr::null()) };
        // CTFont is toll-free bridged with NSFont.
        let ptr: NonNull<NSFont> = CFRetained::as_ptr(&ct).cast();
        unsafe { Retained::retain(ptr.as_ptr()) }.unwrap()
    })
}

fn colors() -> (Retained<NSColor>, Retained<NSColor>) {
    if light() {
        (NSColor::blackColor(), NSColor::whiteColor())
    } else {
        (NSColor::whiteColor(), NSColor::blackColor())
    }
}

fn render(doc: &Doc) {
    let size = size();
    let (fg, bg) = colors();
    let s = NSString::from_str(&doc.content);
    let attr = NSMutableAttributedString::initWithString(NSMutableAttributedString::alloc(), &s);
    let range = NSRange::new(0, s.length());
    let para = NSMutableParagraphStyle::new();
    para.setMinimumLineHeight(size);
    para.setMaximumLineHeight(size);
    let f = font(size);
    unsafe {
        attr.addAttribute_value_range(NSFontAttributeName, &f, range);
        attr.addAttribute_value_range(NSForegroundColorAttributeName, &fg, range);
        attr.addAttribute_value_range(NSParagraphStyleAttributeName, &para, range);
    }
    let tv = &doc.text;
    if let Some(storage) = unsafe { tv.textStorage() } {
        storage.setAttributedString(&attr);
    }
    tv.setBackgroundColor(&bg);
    tv.setInsertionPointColor(Some(&fg));
    if let Some(scroll) = tv.enclosingScrollView() {
        scroll.setBackgroundColor(&bg);
    }
    doc.window.setBackgroundColor(Some(&bg));
    let name = if light() { unsafe { NSAppearanceNameAqua } } else { unsafe { NSAppearanceNameDarkAqua } };
    doc.window.setAppearance(NSAppearance::appearanceNamed(name).as_deref());
}

/// Window content size that fits the text (at least 80 columns), capped to the screen.
fn fit_size(doc: &Doc, mtm: MainThreadMarker) -> NSSize {
    let size = size();
    let (cols, lines) = nfo_core::dimensions(&doc.content);
    let w = cols.max(MIN_COLS) as f64 * size / 2.0 + INSET * 2.0 + 10.0;
    let h = lines as f64 * size + INSET * 2.0;
    let screen = doc.window.screen().or_else(|| objc2_app_kit::NSScreen::mainScreen(mtm));
    let max = screen.map(|s| s.visibleFrame().size).unwrap_or(NSSize::new(1440.0, 900.0));
    NSSize::new(w.min(max.width - 40.0), h.min(max.height - 60.0).max(200.0))
}

fn fit_window(doc: &Doc, mtm: MainThreadMarker) {
    let content = fit_size(doc, mtm);
    let old = doc.window.frame();
    let mut frame = doc.window.frameRectForContentRect(NSRect::new(NSPoint::new(0.0, 0.0), content));
    frame.origin = NSPoint::new(old.origin.x, old.origin.y + old.size.height - frame.size.height);
    doc.window.setFrame_display_animate(frame, true, false);
}

fn render_all(refit: bool) {
    let mtm = MainThreadMarker::new().unwrap();
    DOCS.with_borrow(|docs| {
        for d in docs {
            render(d);
            if refit {
                fit_window(d, mtm);
            }
        }
    });
}

// ---------- documents ----------

fn open_path(path: &Path, delegate: &AppDelegate, mtm: MainThreadMarker) {
    if let Some(w) = DOCS.with_borrow(|docs| {
        docs.iter()
            .find(|d| d.window.representedFilename().to_string() == path.to_string_lossy())
            .map(|d| d.window.clone())
    }) {
        w.makeKeyAndOrderFront(None);
        return;
    }
    let Ok(bytes) = std::fs::read(path) else { return };
    let content = nfo_core::decode(&bytes);

    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable;
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(700.0, 500.0)),
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitleWithRepresentedFilename(&NSString::from_str(&path.to_string_lossy()));
    window.setDelegate(Some(ProtocolObject::from_ref(delegate)));
    window.setTabbingMode(objc2_app_kit::NSWindowTabbingMode::Disallowed);

    let scroll: Retained<NSScrollView> = NSTextView::scrollableTextView(mtm);
    scroll.setHasHorizontalScroller(true);
    scroll.setAutohidesScrollers(true);
    scroll.setBorderType(objc2_app_kit::NSBorderType::NoBorder);
    let text: Retained<NSTextView> = scroll.documentView().unwrap().downcast().unwrap();
    text.setEditable(false);
    text.setSelectable(true);
    text.setRichText(false);
    text.setTextContainerInset(NSSize::new(INSET, INSET));
    text.setHorizontallyResizable(true);
    text.setMaxSize(NSSize::new(f64::MAX, f64::MAX));
    text.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
    if let Some(tc) = unsafe { text.textContainer() } {
        tc.setWidthTracksTextView(false);
        tc.setContainerSize(NSSize::new(f64::MAX, f64::MAX));
        tc.setLineFragmentPadding(0.0);
    }
    window.setContentView(Some(&scroll));

    let doc = Doc { window: window.clone(), text, content };
    render(&doc);
    let content_size = fit_size(&doc, mtm);
    window.setContentSize(content_size);
    let start = CASCADE.get();
    if start.x == 0.0 && start.y == 0.0 {
        window.center();
        let f = window.frame();
        CASCADE.set(NSPoint::new(f.origin.x, f.origin.y + f.size.height));
    }
    CASCADE.set(window.cascadeTopLeftFromPoint(CASCADE.get()));
    window.makeKeyAndOrderFront(None);
    DOCS.with_borrow_mut(|docs| docs.push(doc));
}

fn show_open_panel(delegate: &AppDelegate, mtm: MainThreadMarker) {
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setAllowsMultipleSelection(true);
    panel.setCanChooseDirectories(false);
    if panel.runModal() == NSModalResponseOK {
        for url in panel.URLs() {
            if let Some(p) = url.to_file_path() {
                open_path(&p, delegate, mtm);
            }
        }
    }
    if DOCS.with_borrow(|d| d.is_empty()) {
        NSApplication::sharedApplication(mtm).terminate(None);
    }
}

// ---------- delegate ----------

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "NFOAppDelegate"]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _n: &NSNotification) {
            unsafe {
                let _: () = msg_send![self, performSelector: sel!(openIfEmpty), withObject: None::<&AnyObject>, afterDelay: 0.25f64];
            }
        }

        #[unsafe(method(application:openURLs:))]
        fn open_urls(&self, _app: &NSApplication, urls: &NSArray<NSURL>) {
            let mtm = self.mtm();
            for url in urls {
                if let Some(p) = url.to_file_path() {
                    open_path(&p, self, mtm);
                }
            }
        }

        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn quit_after_last(&self, _app: &NSApplication) -> bool {
            true
        }
    }

    unsafe impl NSWindowDelegate for AppDelegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, n: &NSNotification) {
            let Some(obj) = n.object() else { return };
            let ptr = Retained::as_ptr(&obj) as *const AnyObject;
            DOCS.with_borrow_mut(|docs| {
                docs.retain(|d| Retained::as_ptr(&d.window) as *const AnyObject != ptr)
            });
        }
    }

    impl AppDelegate {
        #[unsafe(method(openIfEmpty))]
        fn open_if_empty(&self) {
            if DOCS.with_borrow(|d| d.is_empty()) {
                show_open_panel(self, self.mtm());
            }
        }

        #[unsafe(method(openDocument:))]
        fn open_document(&self, _sender: Option<&AnyObject>) {
            show_open_panel(self, self.mtm());
        }

        #[unsafe(method(invertColors:))]
        fn invert_colors(&self, _sender: Option<&AnyObject>) {
            defaults().setBool_forKey(!light(), &NSString::from_str(PREF_LIGHT));
            render_all(false);
        }

        #[unsafe(method(zoomIn:))]
        fn zoom_in(&self, _sender: Option<&AnyObject>) {
            step_size(1);
        }

        #[unsafe(method(zoomOut:))]
        fn zoom_out(&self, _sender: Option<&AnyObject>) {
            step_size(-1);
        }

        #[unsafe(method(zoomReset:))]
        fn zoom_reset(&self, _sender: Option<&AnyObject>) {
            set_size(DEFAULT_SIZE);
        }

        #[unsafe(method(showAbout:))]
        fn show_about(&self, _sender: Option<&AnyObject>) {
            show_about(self.mtm());
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

fn set_size(s: f64) {
    defaults().setDouble_forKey(s, &NSString::from_str(PREF_SIZE));
    render_all(true);
}

fn step_size(dir: isize) {
    let i = SIZES.iter().position(|&s| s == size()).unwrap_or(1) as isize;
    let j = (i + dir).clamp(0, SIZES.len() as isize - 1) as usize;
    set_size(SIZES[j]);
}

fn show_about(mtm: MainThreadMarker) {
    let text = "github.com/jamzu98\n\nFont: Px437 IBM VGA by VileR\nCC BY-SA 4.0";
    let s = NSString::from_str(text);
    let attr = NSMutableAttributedString::initWithString(NSMutableAttributedString::alloc(), &s);
    let all = NSRange::new(0, s.length());
    let para = NSMutableParagraphStyle::new();
    para.setAlignment(NSTextAlignment::Center);
    let small = NSFont::systemFontOfSize(11.0);
    let gray = NSColor::secondaryLabelColor();
    let url = NSURL::URLWithString(ns_string!("https://github.com/jamzu98")).unwrap();
    unsafe {
        attr.addAttribute_value_range(NSParagraphStyleAttributeName, &para, all);
        attr.addAttribute_value_range(NSFontAttributeName, &small, all);
        attr.addAttribute_value_range(NSForegroundColorAttributeName, &gray, all);
        attr.addAttribute_value_range(NSLinkAttributeName, &url, NSRange::new(0, 18));
    }
    let opts: Retained<NSDictionary<NSString, AnyObject>> =
        NSDictionary::from_slices(&[unsafe { NSAboutPanelOptionCredits }], &[&*attr as &AnyObject]);
    let app = NSApplication::sharedApplication(mtm);
    unsafe { app.orderFrontStandardAboutPanelWithOptions(&opts) };
    app.activate();
}

// ---------- menus ----------

fn item(mtm: MainThreadMarker, title: &str, action: Option<Sel>, key: &str) -> Retained<NSMenuItem> {
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            action,
            &NSString::from_str(key),
        )
    }
}

fn submenu(mtm: MainThreadMarker, bar: &NSMenu, title: &str, items: Vec<Retained<NSMenuItem>>) -> Retained<NSMenu> {
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
    for i in items {
        menu.addItem(&i);
    }
    let holder = item(mtm, title, None, "");
    holder.setSubmenu(Some(&menu));
    bar.addItem(&holder);
    menu
}

fn build_menu(mtm: MainThreadMarker) {
    let app = NSApplication::sharedApplication(mtm);
    let bar = NSMenu::new(mtm);
    let sep = || NSMenuItem::separatorItem(mtm);

    let hide_others = item(mtm, "Hide Others", Some(sel!(hideOtherApplications:)), "h");
    hide_others.setKeyEquivalentModifierMask(NSEventModifierFlags::Command | NSEventModifierFlags::Option);
    submenu(mtm, &bar, "Nether File Opener", vec![
        item(mtm, "About Nether File Opener", Some(sel!(showAbout:)), ""),
        sep(),
        item(mtm, "Hide Nether File Opener", Some(sel!(hide:)), "h"),
        hide_others,
        item(mtm, "Show All", Some(sel!(unhideAllApplications:)), ""),
        sep(),
        item(mtm, "Quit Nether File Opener", Some(sel!(terminate:)), "q"),
    ]);
    submenu(mtm, &bar, "File", vec![
        item(mtm, "Open…", Some(sel!(openDocument:)), "o"),
        sep(),
        item(mtm, "Close", Some(sel!(performClose:)), "w"),
    ]);
    submenu(mtm, &bar, "Edit", vec![
        item(mtm, "Copy", Some(sel!(copy:)), "c"),
        item(mtm, "Select All", Some(sel!(selectAll:)), "a"),
    ]);
    let invert = item(mtm, "Invert Colors", Some(sel!(invertColors:)), "i");
    invert.setKeyEquivalentModifierMask(NSEventModifierFlags::Command | NSEventModifierFlags::Shift);
    submenu(mtm, &bar, "View", vec![
        invert,
        sep(),
        item(mtm, "Zoom In", Some(sel!(zoomIn:)), "+"),
        item(mtm, "Zoom Out", Some(sel!(zoomOut:)), "-"),
        item(mtm, "Actual Size", Some(sel!(zoomReset:)), "0"),
    ]);
    let window = submenu(mtm, &bar, "Window", vec![
        item(mtm, "Minimize", Some(sel!(performMiniaturize:)), "m"),
        item(mtm, "Zoom", Some(sel!(performZoom:)), ""),
    ]);
    app.setMainMenu(Some(&bar));
    app.setWindowsMenu(Some(&window));
}

fn main() {
    let mtm = MainThreadMarker::new().unwrap();
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    let delegate = AppDelegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    build_menu(mtm);
    app.run();
}
