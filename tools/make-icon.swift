#!/usr/bin/env swift
//
// make-icon.swift
//
// Renders the CodeplugMe app icon (a handheld transceiver silhouette on a
// rounded-square "squircle" background) programmatically with CoreGraphics
// and writes every PNG the macOS AppIcon.appiconset needs directly into
// CodeplugMe/Assets.xcassets/AppIcon.appiconset/. Each size is drawn natively
// at its own pixel resolution (not downscaled from one master image), so
// edges stay crisp at every scale.
//
// Run from the repo root:
//   swift tools/make-icon.swift
//
// Deterministic and idempotent: re-running overwrites the existing PNGs with
// the same output. Edit the geometry/color constants below and re-run to
// tweak the design.

import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

// MARK: - Output targets

/// One PNG the appiconset needs: its pixel size and destination filename.
struct IconTarget {
    let pixels: Int
    let filename: String
}

let targets: [IconTarget] = [
    IconTarget(pixels: 16, filename: "icon_16x16.png"),
    IconTarget(pixels: 32, filename: "icon_16x16@2x.png"),
    IconTarget(pixels: 32, filename: "icon_32x32.png"),
    IconTarget(pixels: 64, filename: "icon_32x32@2x.png"),
    IconTarget(pixels: 128, filename: "icon_128x128.png"),
    IconTarget(pixels: 256, filename: "icon_128x128@2x.png"),
    IconTarget(pixels: 256, filename: "icon_256x256.png"),
    IconTarget(pixels: 512, filename: "icon_256x256@2x.png"),
    IconTarget(pixels: 512, filename: "icon_512x512.png"),
    IconTarget(pixels: 1024, filename: "icon_512x512@2x.png"),
]

let outputDir = "CodeplugMe/Assets.xcassets/AppIcon.appiconset"

// MARK: - Palette

let bgTop = CGColor(red: 0x3D / 255.0, green: 0x6E / 255.0, blue: 0x8E / 255.0, alpha: 1)
let bgBottom = CGColor(red: 0x16 / 255.0, green: 0x21 / 255.0, blue: 0x2C / 255.0, alpha: 1)
/// The radio's case. Light, for contrast against the dark background.
let bodyFill = CGColor(red: 0xEC / 255.0, green: 0xEF / 255.0, blue: 0xF3 / 255.0, alpha: 1)
let bodyShade = CGColor(red: 0xB4 / 255.0, green: 0xBE / 255.0, blue: 0xC9 / 255.0, alpha: 1)
/// The LCD. Dark and high-contrast: an unlit light-colored panel is what made
/// an earlier draft read as a sheet of paper rather than a device.
let screenDark = CGColor(red: 0x14 / 255.0, green: 0x1C / 255.0, blue: 0x24 / 255.0, alpha: 1)
let screenGlow = CGColor(red: 0x4F / 255.0, green: 0xC3 / 255.0, blue: 0xD6 / 255.0, alpha: 1)
/// Recessed speaker slots, cut into the case rather than laid on top of it.
let grilleFill = CGColor(red: 0x8A / 255.0, green: 0x97 / 255.0, blue: 0xA5 / 255.0, alpha: 1)
let highlight = CGColor(red: 1, green: 1, blue: 1, alpha: 0.12)

// MARK: - Geometry helpers

/// Builds a rounded-rectangle path from corners expressed as fractions of
/// `size` (0...1, origin at bottom-left, y-up) so the same drawing code
/// produces identical proportions at every pixel resolution.
func roundedRect(_ x0: CGFloat, _ y0: CGFloat, _ x1: CGFloat, _ y1: CGFloat,
                  radiusFraction: CGFloat, size: CGFloat) -> CGPath {
    let rect = CGRect(x: x0 * size, y: y0 * size, width: (x1 - x0) * size, height: (y1 - y0) * size)
    let radius = min(radiusFraction * size, rect.width / 2, rect.height / 2)
    return CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
}

// MARK: - Drawing

/// Draws the full icon (background squircle + HT radio glyph) into `ctx` at
/// `size` pixels square. All proportions are relative to `size` so the same
/// function renders correctly from 16px up to 1024px.
func drawIcon(into ctx: CGContext, size: CGFloat) {
    let colorSpace = CGColorSpaceCreateDeviceRGB()

    // --- Background squircle -------------------------------------------------
    let squirclePath = roundedRect(0.085, 0.085, 0.915, 0.915, radiusFraction: 0.18, size: size)
    ctx.saveGState()
    ctx.addPath(squirclePath)
    ctx.clip()

    let gradient = CGGradient(colorsSpace: colorSpace, colors: [bgTop, bgBottom] as CFArray, locations: [0, 1])!
    ctx.drawLinearGradient(gradient,
                            start: CGPoint(x: size * 0.5, y: size * 0.915),
                            end: CGPoint(x: size * 0.5, y: size * 0.085),
                            options: [])

    // Soft top highlight for a touch of depth.
    let sheen = CGGradient(colorsSpace: colorSpace,
                            colors: [highlight, CGColor(red: 1, green: 1, blue: 1, alpha: 0)] as CFArray,
                            locations: [0, 1])!
    ctx.drawLinearGradient(sheen,
                            start: CGPoint(x: size * 0.5, y: size * 0.915),
                            end: CGPoint(x: size * 0.5, y: size * 0.55),
                            options: [])
    ctx.restoreGState()

    // --- HT radio --------------------------------------------------------
    //
    // Read order at a glance: thick antenna up top-left, a chunky case, a dark
    // LCD, and a push-to-talk bump on the left edge. Those four are what say
    // "handheld radio"; everything else is texture that may vanish at 16pt.
    let bodyX0: CGFloat = 0.325, bodyX1: CGFloat = 0.675
    let bodyY0: CGFloat = 0.17, bodyY1: CGFloat = 0.715

    // Antenna: a thick, near-vertical capsule. A thin diagonal line reads as a
    // pen, so this stays heavy and close to upright.
    ctx.saveGState()
    ctx.setLineCap(.round)
    ctx.setLineWidth(0.046 * size)
    ctx.setStrokeColor(bodyShade)
    ctx.move(to: CGPoint(x: 0.395 * size, y: (bodyY1 - 0.02) * size))
    ctx.addLine(to: CGPoint(x: 0.360 * size, y: 0.885 * size))
    ctx.strokePath()
    ctx.restoreGState()

    // Volume/channel knob, poking up from the top edge on the right.
    ctx.saveGState()
    ctx.setLineCap(.round)
    ctx.setLineWidth(0.040 * size)
    ctx.setStrokeColor(bodyShade)
    ctx.move(to: CGPoint(x: 0.600 * size, y: (bodyY1 - 0.02) * size))
    ctx.addLine(to: CGPoint(x: 0.600 * size, y: 0.775 * size))
    ctx.strokePath()
    ctx.restoreGState()

    // Push-to-talk bump on the left edge — a strong, cheap radio signifier that
    // also breaks the plain rectangle that made this read as a sheet of paper.
    let pttPath = roundedRect(0.285, 0.395, 0.345, 0.565, radiusFraction: 0.022, size: size)
    ctx.addPath(pttPath)
    ctx.setFillColor(bodyShade)
    ctx.fillPath()

    // Case, over a slightly larger dark plate so it reads as having depth.
    let shadowPath = roundedRect(bodyX0 - 0.014, bodyY0 - 0.016, bodyX1 + 0.014, bodyY1 - 0.004,
                                  radiusFraction: 0.062, size: size)
    ctx.addPath(shadowPath)
    ctx.setFillColor(bodyShade)
    ctx.fillPath()

    let bodyPath = roundedRect(bodyX0, bodyY0, bodyX1, bodyY1, radiusFraction: 0.055, size: size)
    ctx.addPath(bodyPath)
    ctx.setFillColor(bodyFill)
    ctx.fillPath()

    // LCD: the largest single feature, and dark so the case reads as a device.
    let screenPath = roundedRect(0.370, 0.505, 0.630, 0.675, radiusFraction: 0.020, size: size)
    ctx.addPath(screenPath)
    ctx.setFillColor(screenDark)
    ctx.fillPath()

    // A lit band inside the LCD, standing in for a channel readout.
    let readoutPath = roundedRect(0.398, 0.582, 0.602, 0.640, radiusFraction: 0.014, size: size)
    ctx.addPath(readoutPath)
    ctx.setFillColor(screenGlow)
    ctx.fillPath()

    // Speaker grille: two short recessed slots below the display. Kept narrow
    // and few — full-width evenly spaced bars read as ruled lines of text.
    for (y0, y1) in [(0.395, 0.435), (0.310, 0.350)] as [(CGFloat, CGFloat)] {
        let barPath = roundedRect(0.410, y0, 0.590, y1, radiusFraction: 0.020, size: size)
        ctx.addPath(barPath)
        ctx.setFillColor(grilleFill)
        ctx.fillPath()
    }
}

/// Renders `drawIcon` into a fresh premultiplied-alpha RGBA bitmap of
/// `pixels`x`pixels` and returns the resulting CGImage.
func renderImage(pixels: Int) -> CGImage {
    let colorSpace = CGColorSpaceCreateDeviceRGB()
    let ctx = CGContext(data: nil,
                         width: pixels,
                         height: pixels,
                         bitsPerComponent: 8,
                         bytesPerRow: 0,
                         space: colorSpace,
                         bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    drawIcon(into: ctx, size: CGFloat(pixels))
    return ctx.makeImage()!
}

/// Writes `image` to `path` as a PNG.
func writePNG(_ image: CGImage, to path: String) {
    let url = URL(fileURLWithPath: path) as CFURL
    guard let dest = CGImageDestinationCreateWithURL(url, UTType.png.identifier as CFString, 1, nil) else {
        FileHandle.standardError.write("failed to create PNG destination for \(path)\n".data(using: .utf8)!)
        exit(1)
    }
    CGImageDestinationAddImage(dest, image, nil)
    if !CGImageDestinationFinalize(dest) {
        FileHandle.standardError.write("failed to write PNG \(path)\n".data(using: .utf8)!)
        exit(1)
    }
}

// MARK: - Main

try? FileManager.default.createDirectory(atPath: outputDir, withIntermediateDirectories: true)

for target in targets {
    let image = renderImage(pixels: target.pixels)
    let path = "\(outputDir)/\(target.filename)"
    writePNG(image, to: path)
    print("wrote \(path) (\(target.pixels)x\(target.pixels))")
}

print("done: \(targets.count) icons written to \(outputDir)")
