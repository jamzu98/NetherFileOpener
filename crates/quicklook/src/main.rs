//! Quick Look preview extension for .nfo files.
//! Renders the text with the bundled DOS font into a PDF (exactly like the app).

use std::ffi::{c_char, c_int, CString};
use std::ptr::NonNull;

use block2::{DynBlock, RcBlock};
use objc2::rc::Retained;
use objc2::{define_class, AnyThread, ClassType};
use objc2_core_foundation::{
    kCFBooleanTrue, CFAttributedString, CFData, CFDictionary, CFMutableData,
    CFPreferencesGetAppBooleanValue, CFRetained, CFString, CFType, CGFloat, CGPoint, CGRect,
    CGSize,
};
use objc2_core_graphics::{
    CGContext, CGDataConsumer, CGPDFContextBeginPage, CGPDFContextClose, CGPDFContextCreate,
    CGPDFContextEndPage,
};
use objc2_core_text::{
    kCTFontAttributeName, kCTForegroundColorFromContextAttributeName, CTFont,
    CTFontManagerCreateFontDescriptorFromData, CTLine,
};
use objc2_foundation::{NSData, NSError, NSObject, NSObjectProtocol};
use objc2_quick_look_ui::{
    QLFilePreviewRequest, QLPreviewProvider, QLPreviewReply, QLPreviewingController,
};
use objc2_uniform_type_identifiers::UTTypePDF;

const PAD: CGFloat = 16.0;
const CELL_W: CGFloat = 8.0;
const CELL_H: CGFloat = 16.0;

fn light_mode() -> bool {
    let key = CFString::from_str(nfo_core::PREF_LIGHT);
    let app = CFString::from_str(nfo_core::APP_BUNDLE_ID);
    unsafe { CFPreferencesGetAppBooleanValue(&key, &app, std::ptr::null_mut()) }
}

fn page_size(text: &str) -> CGSize {
    let (cols, lines) = nfo_core::dimensions(text);
    CGSize::new(
        cols.max(80) as CGFloat * CELL_W + PAD * 2.0,
        lines as CGFloat * CELL_H + PAD * 2.0,
    )
}

/// Draw the text into a single-page PDF.
fn render_pdf(text: &str, light: bool) -> Option<CFRetained<CFMutableData>> {
    let size = page_size(text);
    let data = CFMutableData::new(None, 0)?;
    let consumer = CGDataConsumer::with_cf_data(Some(&data))?;
    let media = CGRect::new(CGPoint::new(0.0, 0.0), size);
    let ctx = unsafe { CGPDFContextCreate(Some(&consumer), &media, None) }?;
    let c = Some(&*ctx);

    let desc = unsafe { CTFontManagerCreateFontDescriptorFromData(&CFData::from_bytes(nfo_core::FONT_TTF)) }?;
    let font = unsafe { CTFont::with_font_descriptor(&desc, CELL_H, std::ptr::null()) };
    let descent = unsafe { font.descent() };
    let yes: &CFType = unsafe { kCFBooleanTrue }?;
    let keys: [&CFString; 2] = unsafe { [kCTFontAttributeName, kCTForegroundColorFromContextAttributeName] };
    let values: [&CFType; 2] = [&font, yes];
    let attrs = CFDictionary::<CFString, CFType>::from_slices(&keys, &values);

    let (fg, bg) = if light { (0.0, 1.0) } else { (1.0, 0.0) };
    unsafe { CGPDFContextBeginPage(c, None) };
    CGContext::set_rgb_fill_color(c, bg, bg, bg, 1.0);
    CGContext::fill_rect(c, media);
    CGContext::set_rgb_fill_color(c, fg, fg, fg, 1.0);
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let s = CFString::from_str(line);
        let Some(attr) = (unsafe { CFAttributedString::new(None, Some(&s), Some(attrs.as_opaque())) }) else {
            continue;
        };
        let ct_line = unsafe { CTLine::with_attributed_string(&attr) };
        let baseline = size.height - PAD - (i as CGFloat + 1.0) * CELL_H + descent;
        CGContext::set_text_position(c, PAD, baseline);
        unsafe { ct_line.draw(&ctx) };
    }
    CGPDFContextEndPage(c);
    CGPDFContextClose(c);
    Some(data)
}

define_class!(
    #[unsafe(super(QLPreviewProvider, NSObject))]
    #[name = "NFOPreviewProvider"]
    struct PreviewProvider;

    unsafe impl NSObjectProtocol for PreviewProvider {}

    unsafe impl QLPreviewingController for PreviewProvider {
        #[unsafe(method(providePreviewForFileRequest:completionHandler:))]
        fn provide_preview(
            &self,
            request: &QLFilePreviewRequest,
            handler: &DynBlock<dyn Fn(*mut QLPreviewReply, *mut NSError)>,
        ) {
            let url = unsafe { request.fileURL() };
            let bytes = url
                .to_file_path()
                .and_then(|p| std::fs::read(p).ok())
                .unwrap_or_default();
            let text = nfo_core::decode(&bytes);
            let size = page_size(&text);
            let light = light_mode();

            let block = RcBlock::new(move |_reply: NonNull<QLPreviewReply>, _err: *mut *mut NSError| {
                let bytes = render_pdf(&text, light)
                    .map(|d| unsafe { d.as_bytes_unchecked() }.to_vec())
                    .unwrap_or_default();
                Retained::autorelease_return(NSData::with_bytes(&bytes))
            });
            let reply = unsafe {
                QLPreviewReply::initWithDataOfContentType_contentSize_dataCreationBlock(
                    QLPreviewReply::alloc(),
                    UTTypePDF,
                    size,
                    &block,
                )
            };
            handler.call((Retained::as_ptr(&reply) as *mut QLPreviewReply, std::ptr::null_mut()));
        }
    }
);

unsafe extern "C" {
    fn NSExtensionMain(argc: c_int, argv: *const *const c_char) -> c_int;
}

fn main() {
    // Register the principal class before the extension host looks it up.
    let _ = PreviewProvider::class();

    let args: Vec<CString> = std::env::args().map(|a| CString::new(a).unwrap()).collect();
    let mut argv: Vec<*const c_char> = args.iter().map(|a| a.as_ptr()).collect();
    argv.push(std::ptr::null());
    let code = unsafe { NSExtensionMain(args.len() as c_int, argv.as_ptr()) };
    std::process::exit(code);
}
