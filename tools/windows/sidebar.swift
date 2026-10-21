// Draws the Windows installer's side picture (NSIS welcome and finish pages, 164×314 at 1× and
// 2×): the app icon, «مصمم» and "Mosamem" on the interface's dark graphite.
//   swift tools/windows/sidebar.swift <out.png> <scale>
// tools/windows/make-sidebar.sh turns it into the 24-bit BMP that NSIS takes.
import AppKit
import CoreText

let args = CommandLine.arguments
let out = args[1]
let k = CGFloat(Double(args.count > 2 ? args[2] : "1") ?? 1)
let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
let (W, H) = (Int(164 * k), Int(314 * k))
let cs = CGColorSpaceCreateDeviceRGB()
let ctx = CGContext(data: nil, width: W, height: H, bitsPerComponent: 8, bytesPerRow: 0, space: cs, bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)!
// CoreGraphics' origin is at the bottom: y measured from the top is H − y
func top(_ y: CGFloat) -> CGFloat { CGFloat(H) - y * k }

// background: the interface's graphite, a little lighter toward the bottom
let bg = CGGradient(colorsSpace: cs, colors: [CGColor(srgbRed: 0.086, green: 0.094, blue: 0.114, alpha: 1), CGColor(srgbRed: 0.141, green: 0.149, blue: 0.173, alpha: 1)] as CFArray, locations: [0, 1])!
ctx.drawLinearGradient(bg, start: CGPoint(x: 0, y: CGFloat(H)), end: CGPoint(x: 0, y: 0), options: [])

// the icon
let icon = NSImage(contentsOf: root.appendingPathComponent("assets/icon.png"))!
var rect = CGRect(x: 0, y: 0, width: icon.size.width, height: icon.size.height)
let cg = icon.cgImage(forProposedRect: &rect, context: nil, hints: nil)!
let s = 88 * k
ctx.interpolationQuality = .high
ctx.draw(cg, in: CGRect(x: (CGFloat(W) - s) / 2, y: top(52) - s, width: s, height: s))

// a line of text, centered, `y` = its baseline from the top
func text(_ str: String, font: String, size: CGFloat, color: CGColor, y: CGFloat) {
  let f = CTFontCreateWithName(font as CFString, size * k, nil)
  let a = NSAttributedString(string: str, attributes: [.font: f, .foregroundColor: color])
  let line = CTLineCreateWithAttributedString(a)
  let w = CTLineGetTypographicBounds(line, nil, nil, nil)
  ctx.textPosition = CGPoint(x: (CGFloat(W) - CGFloat(w)) / 2, y: top(y))
  CTLineDraw(line, ctx)
}
// the bundled IBM Plex Sans Arabic, registered for this process
for file in ["IBMPlexSansArabic-Bold.ttf", "IBMPlexSansArabic-Regular.ttf"] {
  CTFontManagerRegisterFontsForURL(root.appendingPathComponent("assets/fonts/ibm-plex-sans-arabic/\(file)") as CFURL, .process, nil)
}
text("مصمم", font: "IBMPlexSansArabic-Bold", size: 34, color: CGColor(srgbRed: 0.95, green: 0.96, blue: 0.97, alpha: 1), y: 200)
text("Mosamem", font: "IBMPlexSansArabic-Regular", size: 15, color: CGColor(srgbRed: 0.6, green: 0.63, blue: 0.68, alpha: 1), y: 226)
// the accent
ctx.setFillColor(CGColor(srgbRed: 0.247, green: 0.769, blue: 0.667, alpha: 1))
ctx.fill(CGRect(x: (CGFloat(W) - 28 * k) / 2, y: top(250), width: 28 * k, height: 3 * k))

let img = ctx.makeImage()!
let rep = NSBitmapImageRep(cgImage: img)
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
