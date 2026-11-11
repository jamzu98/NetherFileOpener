// Generates assets/AppIcon.icns. Run: swift tools/make-icon.swift
import AppKit
import CoreText

let root = URL(fileURLWithPath: CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : ".")
let fontData = try! Data(contentsOf: root.appendingPathComponent("assets/Px437_IBM_VGA_8x16.ttf"))
let desc = CTFontManagerCreateFontDescriptorFromData(fontData as CFData)!

func draw(_ px: Int) -> Data {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px, bitsPerSample: 8,
                               samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    let ctx = NSGraphicsContext(bitmapImageRep: rep)!
    NSGraphicsContext.current = ctx
    ctx.imageInterpolation = .none
    ctx.shouldAntialias = true
    let s = CGFloat(px) / 1024
    let rect = NSRect(x: 100 * s, y: 100 * s, width: 824 * s, height: 824 * s)
    let path = NSBezierPath(roundedRect: rect, xRadius: 185 * s, yRadius: 185 * s)
    NSColor.black.setFill()
    path.fill()
    NSColor(white: 0.25, alpha: 1).setStroke()
    path.lineWidth = max(1, 6 * s)
    path.stroke()

    ctx.shouldAntialias = false
    func text(_ str: String, _ size: CGFloat, _ y: CGFloat, _ color: NSColor) {
        let font = CTFontCreateWithFontDescriptor(desc, size * s, nil) as NSFont
        let a = NSAttributedString(string: str, attributes: [.font: font, .foregroundColor: color])
        let w = a.size().width
        a.draw(at: NSPoint(x: (CGFloat(px) - w) / 2, y: y * s))
    }
    text("NFO", 288, 430, .white)
    text("░▒▓█▓▒░", 96, 300, NSColor(white: 0.85, alpha: 1))
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

let iconset = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("AppIcon.iconset")
try? FileManager.default.removeItem(at: iconset)
try! FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
for base in [16, 32, 128, 256, 512] {
    try! draw(base).write(to: iconset.appendingPathComponent("icon_\(base)x\(base).png"))
    try! draw(base * 2).write(to: iconset.appendingPathComponent("icon_\(base)x\(base)@2x.png"))
}
let p = Process()
p.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
p.arguments = ["-c", "icns", iconset.path, "-o", root.appendingPathComponent("assets/AppIcon.icns").path]
try! p.run(); p.waitUntilExit()
print("assets/AppIcon.icns")
