//! Metal shader wallpapers.
//!
//! Same window plumbing as the video path, but instead of an `AVPlayerLayer`
//! each screen gets a `CAMetalLayer` that a background thread draws a fragment
//! shader into every frame.  The shader sees the ShaderToy-style uniforms
//! `iTime` / `iResolution` / `iFrame` (named `time`, `resolution`, `frame`
//! here), so procedural backgrounds need no video file at all.

use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{msg_send, MainThreadOnly};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSApplication, NSApplicationActivationPolicy, NSBackingStoreType,
    NSColor, NSScreen, NSView, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSDate, NSRect, NSRunLoop, NSString};
use objc2_metal::{
    MTLClearColor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLDevice, MTLDrawable, MTLLibrary,
    MTLLoadAction, MTLPixelFormat, MTLPrimitiveType, MTLRenderPassDescriptor, MTLRenderCommandEncoder,
    MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLStorageMode, MTLStoreAction, MTLTexture,
    MTLTextureUsage, MTLCreateSystemDefaultDevice,
};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};

use crate::SHOULD_QUIT;

const WINDOW_LEVEL: isize = -10000;

/// Prepended to every user shader.  The user file only has to define
/// `fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]])`.
const PREAMBLE: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct Uniforms {
    float2 resolution;
    float  time;
    float  frame;
    float2 mouse;
};

struct VOut {
    float4 position [[position]];
    float2 uv;
};

vertex VOut v_main(uint vid [[vertex_id]]) {
    float2 p[3] = { float2(-1.0, -1.0), float2(3.0, -1.0), float2(-1.0, 3.0) };
    VOut o;
    o.position = float4(p[vid], 0.0, 1.0);
    o.uv = p[vid] * 0.5 + 0.5;
    return o;
}
"#;

#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms {
    resolution: [f32; 2],
    time: f32,
    frame: f32,
    mouse: [f32; 2],
}

unsafe impl Send for Gpu {}
unsafe impl Sync for Gpu {}

struct Gpu {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
}

// ---- built-ins ------------------------------------------------------------

pub const BUILTINS: &[(&str, &str)] = &[
    (
        "plasma",
        r#"
fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 p = in.uv * 2.0 - 1.0;
    p.x *= u.resolution.x / max(u.resolution.y, 1.0);
    float t = u.time * 0.25;
    float v = sin(p.x * 3.0 + t)
            + sin(p.y * 4.0 - t * 1.3)
            + sin((p.x + p.y) * 2.5 + t * 0.7)
            + sin(length(p) * 6.0 - t * 2.0);
    v *= 0.25;
    float3 col = mix(float3(0.035, 0.045, 0.040), float3(0.09, 0.52, 0.29),
                     smoothstep(-0.55, 0.85, v));
    col += float3(0.015, 0.035, 0.022) * sin(v * 6.2831);
    return float4(col, 1.0);
}
"#,
    ),
    (
        "aurora",
        r#"
static float hash(float2 p) {
    return fract(sin(dot(p, float2(127.1, 311.7))) * 43758.5453);
}
static float noise(float2 p) {
    float2 i = floor(p);
    float2 f = fract(p);
    float2 w = f * f * (3.0 - 2.0 * f);
    float a = hash(i);
    float b = hash(i + float2(1.0, 0.0));
    float c = hash(i + float2(0.0, 1.0));
    float d = hash(i + float2(1.0, 1.0));
    return mix(mix(a, b, w.x), mix(c, d, w.x), w.y);
}

fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 uv = in.uv;
    float aspect = u.resolution.x / max(u.resolution.y, 1.0);
    float2 p = float2(uv.x * aspect, uv.y);
    float t = u.time * 0.06;

    float3 col = mix(float3(0.010, 0.016, 0.030), float3(0.020, 0.040, 0.070), uv.y);

    for (int i = 0; i < 3; i++) {
        float fi = float(i);
        float band = 0.62 - fi * 0.13;
        float wobble = noise(float2(p.x * 2.2 + fi * 4.0, t * 2.0 + fi)) * 0.22;
        float d = abs(p.y - (band + wobble + 0.06 * sin(p.x * 3.1 + t * 6.0 + fi)));
        float glow = exp(-d * 26.0);
        float3 tint = mix(float3(0.05, 0.45, 0.32), float3(0.10, 0.25, 0.55), fi / 2.0);
        col += tint * glow * (0.55 - fi * 0.12);
    }

    col += float3(0.010, 0.020, 0.015) * noise(p * 60.0 + t * 10.0);
    return float4(col, 1.0);
}
"#,
    ),
    (
        "rings",
        r#"
fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 p = in.uv * 2.0 - 1.0;
    p.x *= u.resolution.x / max(u.resolution.y, 1.0);
    float t = u.time * 0.35;
    float r = length(p);

    float rings = sin(r * 22.0 - t * 3.0) * 0.5 + 0.5;
    rings = pow(rings, 3.0);
    float core = exp(-r * 2.4);
    float3 col = float3(0.030, 0.034, 0.032);
    col += float3(0.06, 0.42, 0.24) * rings * (1.0 - smoothstep(0.2, 1.1, r));
    col += float3(0.10, 0.30, 0.20) * core * 0.8;
    col *= 1.0 - smoothstep(0.9, 1.5, r) * 0.7;
    return float4(col, 1.0);
}
"#,
    ),
    (
        "stark",
        r#"
// Stark — a workshop hologram. A projected light-grid floor runs to a glowing
// horizon; above it a gyroscope of tilted rings turns around segmented,
// counter-rotating bands and a hexagonal core, with satellites riding the
// rings. Two data panels hang either side (bar columns, scrolling readouts, a
// live waveform, a gauge), motes of light drift up through the projection, and
// every so often the signal tears and fringes like a real projector would.
static float hash21(float2 p) {
    p = fract(p * float2(234.34, 435.345));
    p += dot(p, p + 34.23);
    return fract(p.x * p.y);
}
static float vnoise(float2 p) {
    float2 i = floor(p);
    float2 f = fract(p);
    float2 w = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + float2(1, 0)), w.x),
               mix(hash21(i + float2(0, 1)), hash21(i + float2(1, 1)), w.x), w.y);
}
static float2 rot(float2 p, float a) {
    float c = cos(a), s = sin(a);
    return float2(c * p.x - s * p.y, s * p.x + c * p.y);
}
static float hair(float d, float w) { return 1.0 - smoothstep(0.0, w, d); }
static float sdBox(float2 p, float2 b) {
    float2 d = abs(p) - b;
    return length(max(d, 0.0)) + min(max(d.x, d.y), 0.0);
}
static float sdChamfer(float2 p, float2 b, float c) {
    return max(sdBox(p, b), (abs(p.x) + abs(p.y) - (b.x + b.y - c)) * 0.7071);
}
static float sdHex(float2 p, float r) {
    const float3 k = float3(-0.866025404, 0.5, 0.577350269);
    p = abs(p);
    p -= 2.0 * min(dot(k.xy, p), 0.0) * k.xy;
    p -= float2(clamp(p.x, -k.z * r, k.z * r), r);
    return length(p) * sign(p.y);
}

// A panel frame: chamfered outline, a header rule, and brackets on two corners.
static float panelFrame(float2 q, float2 b, float px) {
    float f = hair(abs(sdChamfer(q, b, 0.05)), px * 1.4) * 0.55;
    f += hair(abs(q.y - (b.y - 0.07)), px) * step(abs(q.x), b.x - 0.03) * 0.35;
    float2 cq = abs(q) - b - 0.025;
    float br = hair(abs(cq.y), px * 1.6) * step(abs(cq.x + 0.04), 0.04)
             + hair(abs(cq.x), px * 1.6) * step(abs(cq.y + 0.04), 0.04);
    return f + br * 0.9;
}

fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 res = u.resolution;
    float aspect = res.x / max(res.y, 1.0);
    float t = u.time;
    float2 uv = in.uv;
    float px = 2.0 / max(res.y, 1.0);

    // ---- signal tears: a few rows slip sideways for a moment, every ~10 s
    float slot = floor(t * 1.4);
    float tearing = step(0.94, hash21(float2(slot, 7.1)));
    float bandY = hash21(float2(slot, 3.3));
    float inBand = step(abs(uv.y - bandY), 0.03 + 0.05 * hash21(float2(slot, 9.2)));
    float tear = tearing * inBand;
    uv.x += tear * (hash21(float2(slot, floor(uv.y * 60.0))) - 0.5) * 0.06;

    float2 p = (uv - 0.5) * float2(aspect, 1.0) * 2.0;

    float3 cyan  = float3(0.24, 0.84, 1.00);
    float3 ice   = float3(0.86, 0.97, 1.00);
    float3 amber = float3(1.00, 0.34, 0.22);

    // ---- the room: deep blue-black, a slow haze lit by the projection
    float3 col = mix(float3(0.004, 0.008, 0.016), float3(0.010, 0.026, 0.046), smoothstep(-1.0, 0.8, p.y));
    float haze = vnoise(p * 1.3 + float2(t * 0.025, t * 0.01)) * 0.6 + vnoise(p * 3.1 - t * 0.04) * 0.4;
    col += cyan * 0.030 * haze * exp(-length(p - float2(0.0, 0.1)) * 0.9);

    // ---- the light-grid floor, rolling toward you
    float horizon = -0.30;
    float below = horizon - p.y;
    if (below > 0.0) {
        float z = 0.32 / below;
        float2 g = float2(p.x * z * 1.2, z + t * 0.45);
        float2 fw = fwidth(g) * 1.4;
        float2 f = abs(fract(g) - 0.5);
        float lines = max(hair(0.5 - f.x, fw.x), hair(0.5 - f.y, fw.y));
        float fade = exp(-z * 0.22) * smoothstep(0.0, 0.04, below);
        col += cyan * lines * fade * 0.42;
        // the projection's footprint on the floor
        col += cyan * 0.06 * exp(-length(float2(p.x * 0.8, (p.y - horizon) * 3.0)) * 2.0);
    }
    col += cyan * exp(-abs(p.y - horizon) * 70.0) * 0.22 * exp(-abs(p.x) * 0.6);

    // ---- the projection
    float sc = 0.80;
    float2 c = float2(0.0, 0.14);
    float2 q = p - c;
    float r = length(q);
    float a = atan2(q.y, q.x);
    float boot = smoothstep(0.0, 2.5, t);   // rings draw in when it starts

    // segmented bands, alternating direction
    for (int i = 0; i < 5; i++) {
        float fi = float(i);
        float R = sc * (0.20 + fi * 0.085);
        float N = (i % 2 == 0) ? (36.0 + fi * 12.0) : (6.0 + fi * 3.0);
        float speed = ((i % 2 == 0) ? 1.0 : -1.0) * (0.04 + 0.02 * fi);
        float duty = (i % 2 == 0) ? 0.55 : 0.72;
        float seg = step(fract(a / 6.2831853 * N + t * speed), duty);
        seg *= step(fract(a / 6.2831853 + 0.5), boot);
        float d = abs(r - R);
        col += cyan * seg * (hair(d, px * (1.4 + fi * 0.25)) * 0.85 + exp(-d * 70.0) * 0.20);
        // the gaps keep a faint trace of the band, so they read as dashes on a
        // track rather than holes
        col += cyan * (hair(d, px * 1.2) * 0.10 + exp(-d * 70.0) * 0.06);
    }

    // the main ring, with a projector's colour fringe
    {
        float R = sc * 0.66;
        float w = px * 2.2;
        float3 ring = float3(hair(abs(r * 1.004 - R), w), hair(abs(r - R), w), hair(abs(r * 0.996 - R), w));
        col += ring * cyan * 1.1 + cyan * exp(-abs(r - R) * 28.0) * 0.30;
        // an amber arc that blinks, like something needs attention
        float sweep = fract(t * 0.03);
        float da = abs(fract(a / 6.2831853 - sweep + 0.5) - 0.5);
        float blink = 0.55 + 0.45 * step(0.5, fract(t * 1.6));
        col += amber * hair(abs(r - R * 1.035), px * 2.5) * step(da, 0.035) * blink;
    }

    // tick ring: fine ticks, every sixth one longer
    {
        float N = 120.0;
        float ta = fract(a / 6.2831853 * N - t * 0.01);
        float arc = abs(ta - 0.5) / N * 6.2831853 * r;
        float major = step(fract((floor(a / 6.2831853 * N - t * 0.01)) / 6.0), 0.01);
        float R0 = sc * 0.74;
        float len = major > 0.5 ? 0.045 : 0.022;
        float inRing = step(R0, r) * step(r, R0 + len * sc);
        col += cyan * hair(abs(arc - 0.5 / N * 6.2831853 * r), px * 1.2) * inRing * (major > 0.5 ? 0.8 : 0.45);
    }

    // the gyroscope: tilted rings turning about the core, satellites riding them
    for (int k = 0; k < 3; k++) {
        float fk = float(k);
        float phi = t * (0.07 + 0.03 * fk) * ((k == 1) ? -1.0 : 1.0) + fk * 2.1;
        float R = sc * (0.90 + 0.07 * fk);
        float B = R * (0.22 + 0.16 * fk + 0.06 * sin(t * 0.2 + fk));
        float2 e = rot(q, -phi);
        float d = abs((length(e / float2(R, B)) - 1.0) * min(R, B));
        float front = 0.45 + 0.55 * smoothstep(B * 0.3, -B * 0.3, e.y);
        col += cyan * front * (hair(d, px * 1.5) * 0.75 + exp(-d * 90.0) * 0.20);
        float sa = t * (0.45 + 0.15 * fk) + fk * 1.7;
        float2 sp = rot(float2(R * cos(sa), B * sin(sa)), phi);
        float sd = length(q - sp);
        col += ice * exp(-sd * 140.0) * 1.3 + cyan * exp(-sd * 22.0) * 0.25;
    }

    // radar sweep inside the bands
    {
        float sw = t * 0.5;
        float da = fract((a - sw) / 6.2831853);
        col += cyan * pow(1.0 - da, 6.0) * smoothstep(sc * 0.62, sc * 0.16, r) * 0.12;
    }

    // the core: two hexagons turning against each other, and a heart that breathes
    {
        float h1 = sdHex(rot(q, t * 0.15), sc * 0.12);
        float h2 = sdHex(rot(q, -t * 0.32 + 0.52), sc * 0.072);
        col += cyan * (hair(abs(h1), px * 1.8) * 0.95 + exp(-abs(h1) * 45.0) * 0.3);
        col += cyan * hair(abs(h2), px * 1.4) * 0.7;
        float pulse = 0.78 + 0.22 * sin(t * 2.1);
        col += ice * exp(-r * 30.0 / sc) * 1.25 * pulse;
        col += cyan * exp(-r * 6.5 / sc) * 0.28 * pulse;
    }

    // crosshair through the projection
    float cross = exp(-abs(q.y) * 260.0) + exp(-abs(q.x) * 260.0);
    col += cyan * cross * smoothstep(sc * 1.05, sc * 0.30, r) * smoothstep(sc * 0.13, sc * 0.2, r) * 0.30;

    // ---- the left panel: bar columns and a scrolling readout
    {
        float2 L = float2(-aspect + 0.58, 0.16);
        float2 b = float2(0.36, 0.46);
        float2 lq = p - L;
        if (sdBox(lq, b + 0.06) < 0.0) {
            col += cyan * panelFrame(lq, b, px);
            col += cyan * 0.025 * step(sdChamfer(lq, b, 0.05), 0.0);
            // columns
            float cols = 14.0;
            float x = (lq.x + b.x - 0.04) / (2.0 * b.x - 0.08) * cols;
            float ci = floor(x);
            if (ci >= 0.0 && ci < cols && fract(x) < 0.62) {
                float hgt = 0.35 + 0.65 * (0.5 + 0.5 * sin(t * (0.6 + hash21(float2(ci, 1.0))) + ci * 1.3));
                float top = -b.y + 0.05 + hgt * 0.38;
                float on = step(-b.y + 0.05, lq.y) * step(lq.y, top);
                col += cyan * on * mix(0.18, 0.55, smoothstep(-b.y, top, lq.y));
                col += ice * hair(abs(lq.y - top), px * 1.2) * step(-b.y + 0.05, lq.y) * 0.5;
            }
            // readout rows: dashes standing in for text, scrolling up
            float rowH = 0.034;
            float y = (lq.y - 0.02) / rowH + t * 0.6;
            float ri = floor(y);
            if (lq.y > 0.0 && lq.y < b.y - 0.1 && fract(y) > 0.3 && fract(y) < 0.7) {
                float words = lq.x + b.x - 0.04;
                float wi = floor(words / 0.07);
                float lenR = hash21(float2(ri, 5.0)) * 0.6 + 0.3;
                float on = step(fract(words / 0.07), 0.8) * step(words, lenR * (2.0 * b.x - 0.08)) * step(0.0, words);
                float hot = step(0.86, hash21(float2(ri, wi)));
                col += mix(cyan, amber, hot) * on * 0.38;
            }
        }
    }

    // ---- the right panel: a live waveform and a gauge
    {
        float2 Rc = float2(aspect - 0.58, 0.16);
        float2 b = float2(0.36, 0.46);
        float2 rq = p - Rc;
        if (sdBox(rq, b + 0.06) < 0.0) {
            col += cyan * panelFrame(rq, b, px);
            col += cyan * 0.025 * step(sdChamfer(rq, b, 0.05), 0.0);
            // waveform in the top half, over a fine grid
            float2 wq = rq - float2(0.0, 0.17);
            if (abs(wq.x) < b.x - 0.04 && abs(wq.y) < 0.14) {
                float2 gq = abs(fract(wq * 20.0) - 0.5);
                col += cyan * 0.07 * max(hair(0.5 - gq.x, 0.04), hair(0.5 - gq.y, 0.04));
                float x = wq.x;
                float f = 0.07 * sin(x * 14.0 + t * 2.4) * sin(x * 3.3 - t * 0.7)
                        + 0.025 * sin(x * 41.0 - t * 5.0);
                float d = abs(wq.y - f);
                col += cyan * (hair(d, px * 1.8) * 0.9 + exp(-d * 60.0) * 0.25);
            }
            // gauge in the bottom half
            float2 gq = rq - float2(0.0, -0.20);
            float gr = length(gq);
            float ga = atan2(gq.y, gq.x);
            float fill = 0.55 + 0.35 * sin(t * 0.35);
            float span = fract((ga + 3.14159 * 1.25) / 6.2831853);
            float inArc = step(span, 0.75);
            col += cyan * hair(abs(gr - 0.17), px * 1.2) * inArc * 0.4;
            col += mix(cyan, amber, smoothstep(0.82, 0.9, fill))
                 * hair(abs(gr - 0.17), px * 5.0) * step(span, 0.75 * fill) * 0.9;
            col += ice * exp(-gr * 40.0) * 0.4;
        }
    }

    // ---- motes of light drifting up through the projection
    {
        float2 mp = p * 6.0 + float2(0.0, -t * 0.35);
        float2 id = floor(mp);
        float2 fq = fract(mp) - 0.5;
        float h = hash21(id);
        if (h > 0.78) {
            float2 off = float2(hash21(id + 3.1), hash21(id + 7.7)) - 0.5;
            float d = length(fq - off * 0.7);
            float tw = 0.5 + 0.5 * sin(t * (1.0 + h * 3.0) + h * 40.0);
            col += cyan * exp(-d * 30.0) * 0.35 * tw * exp(-length(p - c) * 0.7);
        }
    }

    // ---- the projector: a scan band, scanlines, flicker, fringe on tears
    float scan = exp(-pow((uv.y - fract(-t * 0.06)) * 22.0, 2.0));
    col += cyan * scan * 0.045;
    col *= 0.90 + 0.10 * sin(uv.y * res.y * 1.4);
    col *= 0.95 + 0.05 * sin(t * 7.3) * sin(t * 2.9);
    if (tear > 0.5) {
        col = float3(col.r * 1.6 + 0.02, col.g * 0.85, col.b * 1.25);
        col += cyan * 0.06;
    }
    col += (hash21(uv * res + fract(t)) - 0.5) * 0.012;
    float2 vq = (in.uv - 0.5) * 2.0;
    col *= 1.0 - 0.45 * dot(vq, vq) * 0.5;

    return float4(max(col, 0.0), 1.0);
}
"#,
    ),
    (
        "arcane",
        r#"
// Arcane — a summoning at night. A circle of light lies on the ground in
// perspective: rings of procedural runes turning against each other, a
// seven-point star and a hexagram, all glowing violet. A pillar of light rises
// out of it into a nebula sky, orbs circle the pillar trailing light, and
// sparks spiral up out of the circle and drift off as fireflies.
static float hash21(float2 p) {
    p = fract(p * float2(234.34, 435.345));
    p += dot(p, p + 34.23);
    return fract(p.x * p.y);
}
static float vnoise(float2 p) {
    float2 i = floor(p);
    float2 f = fract(p);
    float2 w = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + float2(1, 0)), w.x),
               mix(hash21(i + float2(0, 1)), hash21(i + float2(1, 1)), w.x), w.y);
}
static float fbm(float2 p) {
    float v = 0.0, a = 0.5;
    for (int i = 0; i < 4; i++) { v += a * vnoise(p); p = p * 2.03 + 17.0; a *= 0.5; }
    return v;
}
static float2 rot(float2 p, float a) {
    float c = cos(a), s = sin(a);
    return float2(c * p.x - s * p.y, s * p.x + c * p.y);
}
static float sdSeg(float2 p, float2 a, float2 b) {
    float2 pa = p - a, ba = b - a;
    float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}
static float hair(float d, float w) { return 1.0 - smoothstep(0.0, w, d); }

// A star polygon {n/k} of radius r: the lines from each vertex to the k-th next.
static float star(float2 p, float r, int n, int k, float turn) {
    float d = 1e3;
    for (int i = 0; i < n; i++) {
        float a0 = turn + 6.2831853 * float(i) / float(n);
        float a1 = turn + 6.2831853 * float(i + k) / float(n);
        d = min(d, sdSeg(p, r * float2(cos(a0), sin(a0)), r * float2(cos(a1), sin(a1))));
    }
    return d;
}

// One rune: a few strokes picked from a 3x3 lattice by the cell's hash.
static float rune(float2 uv, float seed) {
    float d = 1e3;
    for (int s = 0; s < 3; s++) {
        float h = hash21(float2(seed, float(s) * 7.3));
        float h2 = hash21(float2(seed * 1.7, float(s) * 3.1 + 1.0));
        float2 a = floor(float2(h * 3.0, fract(h * 13.0) * 3.0)) / 2.0;
        float2 b = floor(float2(h2 * 3.0, fract(h2 * 11.0) * 3.0)) / 2.0;
        if (length(a - b) > 0.1) d = min(d, sdSeg(uv, a, b));
    }
    return d;
}

fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 res = u.resolution;
    float aspect = res.x / max(res.y, 1.0);
    float t = u.time;
    float2 uv = in.uv;
    float2 p = (uv - 0.5) * float2(aspect, 1.0) * 2.0;
    float px = 2.0 / max(res.y, 1.0);

    float3 violet = float3(0.71, 0.42, 1.00);
    float3 rose   = float3(1.00, 0.48, 0.85);
    float3 spark  = float3(0.55, 0.95, 1.00);
    float3 white  = float3(1.00, 0.95, 1.00);

    // ---- the night: indigo, a nebula of violet and rose, stars
    float3 col = mix(float3(0.020, 0.008, 0.040), float3(0.050, 0.020, 0.090), smoothstep(-1.0, 1.0, p.y));
    float neb = fbm(p * 0.9 + float2(t * 0.012, -t * 0.006));
    float neb2 = fbm(p * 1.7 - float2(t * 0.02, 0.0) + 5.0);
    col += violet * 0.26 * smoothstep(0.30, 0.85, neb) * smoothstep(-0.6, 0.8, p.y);
    col += rose * 0.15 * smoothstep(0.42, 0.9, neb2) * smoothstep(-0.2, 1.0, p.y);
    {
        float2 sp = p * 38.0;
        float2 id = floor(sp);
        float h = hash21(id);
        if (h > 0.965) {
            float2 off = float2(hash21(id + 1.3), hash21(id + 4.1)) - 0.5;
            float d = length(fract(sp) - 0.5 - off * 0.6);
            float tw = 0.5 + 0.5 * sin(t * (1.5 + h * 5.0) + h * 80.0);
            col += white * exp(-d * 18.0) * 0.55 * tw * smoothstep(-0.5, 0.4, p.y);
        }
    }
    // an aurora ribbon across the top
    {
        float y = 0.72 + 0.08 * sin(p.x * 1.3 + t * 0.15) + 0.05 * sin(p.x * 3.1 - t * 0.22);
        float band = exp(-abs(p.y - y) * 9.0) * (0.6 + 0.4 * vnoise(float2(p.x * 3.0 - t * 0.3, 0.0)));
        col += mix(violet, rose, 0.5 + 0.5 * sin(p.x * 0.8 + t * 0.1)) * band * 0.16;
    }

    // ---- the ground: a faint mist the circle lights up
    float ground = smoothstep(-0.25, -0.6, p.y);
    col += violet * 0.05 * ground * fbm(float2(p.x * 2.0 + t * 0.05, p.y * 6.0));

    // ---- the circle, lying on the ground in perspective
    float2 C = float2(0.0, -0.52);
    float2 q = p - C;
    q.y /= 0.38;
    float r = length(q);
    float a = atan2(q.y, q.x);
    float pulse = 0.82 + 0.18 * sin(t * 1.7);
    float w = px * 2.2 / 0.38;
    float glow = 0.0;

    for (int i = 0; i < 4; i++) {
        float R = 0.38 + float(i) * 0.12 + (i == 3 ? 0.04 : 0.0);
        float d = abs(r - R);
        glow += hair(d, w * (i == 3 ? 1.6 : 1.0)) * 0.9 + exp(-d * 40.0) * 0.25;
    }
    // rune bands between the outer rings, turning opposite ways
    for (int b = 0; b < 2; b++) {
        float R0 = (b == 0) ? 0.62 : 0.74;
        float R1 = R0 + 0.10;
        if (r > R0 && r < R1) {
            float N = (b == 0) ? 40.0 : 52.0;
            float turn = (b == 0 ? 1.0 : -1.0) * t * 0.05;
            float ang = fract((a / 6.2831853) + turn) * N;
            float cell = floor(ang);
            float2 ruv = float2(fract(ang), (r - R0) / (R1 - R0));
            ruv = (ruv - 0.5) * float2(1.25, 1.25) + 0.5;
            float d = rune(ruv, cell + float(b) * 100.0);
            float lit = 0.7 + 0.3 * sin(t * 2.0 + cell * 0.7);
            glow += hair(d, 0.05) * lit * 0.8 + exp(-d * 18.0) * lit * 0.12;
        }
    }
    // a seven-point star and a hexagram, turning slowly
    float st = star(q, 0.60, 7, 3, t * 0.06);
    glow += hair(st, w * 1.2) * 0.8 + exp(-st * 50.0) * 0.18;
    float hx = star(q, 0.36, 6, 2, -t * 0.1);
    glow += hair(hx, w * 1.1) * 0.7 + exp(-hx * 60.0) * 0.15;
    glow += exp(-r * 6.0) * 0.6;
    col += mix(violet, rose, smoothstep(0.2, 0.9, r)) * glow * pulse;

    // ---- the pillar of light rising out of it
    float height = p.y - C.y;
    {
        float base = smoothstep(-0.10, 0.03, height);
        height = max(height, 0.0);
        float width = 0.13 + 0.015 * sin(height * 6.0 - t * 2.0);
        col += violet * exp(-pow(p.x / (width * 3.5), 2.0)) * exp(-height * 0.7) * 0.18 * pulse * base;
        float beam = exp(-pow(p.x / width, 2.0)) * exp(-height * 0.9);
        float core = exp(-pow(p.x / (width * 0.18), 2.0)) * exp(-height * 1.4);
        float shimmer = 0.75 + 0.25 * vnoise(float2(p.x * 20.0, height * 4.0 - t * 3.0));
        col += (violet * beam * 0.55 * shimmer * pulse + white * core * 0.6 * pulse) * base;
    }

    // ---- orbs circling the pillar, trailing light
    for (int o = 0; o < 3; o++) {
        float fo = float(o);
        for (int k = 0; k < 7; k++) {
            float tk = t - float(k) * 0.05;
            float ang = tk * (0.55 + fo * 0.12) + fo * 2.094;
            float depth = sin(ang);
            float2 op = float2(cos(ang) * (0.62 - fo * 0.08), C.y + 0.45 + fo * 0.28 + depth * 0.09);
            float d = length(p - op);
            float size = (k == 0) ? 1.0 : 0.55 * (1.0 - float(k) / 7.0);
            float near = 0.55 + 0.45 * (depth * -0.5 + 0.5);
            float3 tint = (o == 1) ? spark : mix(rose, violet, fo * 0.5);
            col += tint * exp(-d * (k == 0 ? 55.0 : 90.0)) * size * near * (k == 0 ? 1.4 : 0.6);
            if (k == 0) col += tint * exp(-d * 9.0) * 0.12 * near;
        }
    }

    // ---- sparks spiralling up out of the circle, becoming fireflies
    for (int l = 0; l < 3; l++) {
        float fl = float(l);
        float scale = 7.0 + fl * 5.0;
        float speed = 0.18 + fl * 0.08;
        float2 mp = p * scale;
        mp.y -= t * speed * scale;
        mp.x += sin(p.y * 3.0 + t * 0.8 + fl) * 0.6;
        float2 id = floor(mp);
        float h = hash21(id + fl * 31.0);
        float2 f = fract(mp) - 0.5;
        float2 off = float2(hash21(id + 2.7), hash21(id + 9.1)) - 0.5;
        float2 d2 = f - off * 0.7;
        float d = length(d2);
        // most sparks live in a cone above the circle; a few drift everywhere
        float cone = exp(-abs(p.x) / (0.25 + max(p.y - C.y, 0.0) * 0.55)) * smoothstep(C.y - 0.1, C.y + 0.1, p.y);
        float keep = step(1.0 - (0.08 + 0.75 * cone), h);
        float tw = 0.5 + 0.5 * sin(t * (2.0 + h * 6.0) + h * 50.0);
        float3 tint = h > 0.97 ? spark : (h > 0.9 ? rose : violet);
        float bright = keep * tw * (0.55 + 1.1 * cone);
        col += tint * exp(-d * 26.0) * bright * 0.9;
        // the bright ones glint as four-point stars
        if (h > 0.93) {
            float glint = exp(-abs(d2.x) * 90.0) * exp(-abs(d2.y) * 9.0) + exp(-abs(d2.y) * 90.0) * exp(-abs(d2.x) * 9.0);
            col += white * glint * bright * 0.45;
        }
    }

    // ---- light on the ground around the circle, and a vignette
    col += violet * 0.10 * exp(-length(float2(p.x * 0.7, (p.y - C.y) * 2.2)) * 1.6) * pulse;
    float2 vq = (in.uv - 0.5) * 2.0;
    col *= 1.0 - 0.4 * dot(vq, vq) * 0.5;
    col += (hash21(uv * res + fract(t)) - 0.5) * 0.008;
    return float4(max(col, 0.0), 1.0);
}
"#,
    ),
    (
        "neon",
        r#"
// Neon — a synthwave city at night. A sunset sky over a huge striped sun; a
// cyberpunk skyline stands in front of it, windows flickering and neon signs
// on the roofs; wireframe mountains frame it; a magenta grid floor rolls toward
// you with the sun reflected in it. Washed in scanlines and grain, with a VHS
// tracking band that rolls through now and then.
static float hash21(float2 p) {
    p = fract(p * float2(234.34, 435.345));
    p += dot(p, p + 34.23);
    return fract(p.x * p.y);
}
static float vnoise(float2 p) {
    float2 i = floor(p);
    float2 f = fract(p);
    float2 w = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + float2(1, 0)), w.x),
               mix(hash21(i + float2(0, 1)), hash21(i + float2(1, 1)), w.x), w.y);
}
static float hair(float d, float w) { return 1.0 - smoothstep(0.0, w, d); }

// A ridge line: jagged, made of a few octaves of noise.
static float ridge(float x, float seed) {
    float h = 0.0, a = 0.5, f = 1.6;
    for (int i = 0; i < 4; i++) {
        h += a * abs(vnoise(float2(x * f, seed)) * 2.0 - 1.0);
        f *= 2.1;
        a *= 0.5;
    }
    return h;
}

fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 res = u.resolution;
    float aspect = res.x / max(res.y, 1.0);
    float t = u.time;
    float2 uv = in.uv;

    // ---- VHS tracking: a band rolling up the picture now and then, tearing it
    float roll = fract(t * 0.045);
    float bandOn = step(0.82, fract(t * 0.045 * 3.0 + 0.3)) ;
    float inBand = exp(-pow((uv.y - roll) * 30.0, 2.0)) * bandOn;
    uv.x += inBand * (vnoise(float2(uv.y * 80.0, t * 20.0)) - 0.5) * 0.04;

    float2 p = (uv - 0.5) * float2(aspect, 1.0) * 2.0;
    float px = 2.0 / max(res.y, 1.0);

    float3 magenta = float3(1.00, 0.18, 0.59);
    float3 cyan    = float3(0.00, 0.94, 1.00);
    float3 yellow  = float3(1.00, 0.82, 0.25);
    float3 orange  = float3(1.00, 0.45, 0.20);
    float3 night   = float3(0.043, 0.016, 0.094);

    float horizon = -0.12;
    float3 col;

    if (p.y > horizon) {
        // ---- the sky: purple-black overhead into magenta and orange at the horizon
        float h = (p.y - horizon) / (1.0 - horizon);
        col = mix(float3(0.55, 0.10, 0.35), float3(0.18, 0.04, 0.26), smoothstep(0.0, 0.35, h));
        col = mix(col, night, smoothstep(0.3, 0.9, h));
        col += orange * 0.35 * exp(-h * 9.0);
        // stars
        float2 sp = p * 30.0;
        float2 id = floor(sp);
        float sh = hash21(id);
        if (sh > 0.97) {
            float2 off = float2(hash21(id + 1.3), hash21(id + 4.1)) - 0.5;
            float d = length(fract(sp) - 0.5 - off * 0.6);
            float tw = 0.5 + 0.5 * sin(t * (1.0 + sh * 4.0) + sh * 60.0);
            col += float3(1.0, 0.85, 1.0) * exp(-d * 16.0) * 0.6 * tw * smoothstep(0.25, 0.7, h);
        }

        // ---- the sun: yellow into magenta, stripes cut out of its lower half,
        // drifting down
        float2 sc = float2(0.0, horizon + 0.36);
        float R = 0.46;
        float d = length(p - sc);
        float sunY = (p.y - sc.y) / R;
        float3 sun = mix(magenta, yellow, smoothstep(-0.9, 0.7, sunY));
        float stripes = 1.0;
        if (sunY < 0.2) {
            float k = (0.2 - sunY);
            float bands = fract((sunY - t * 0.035) * 9.0);
            stripes = step(k * 0.55, bands);
        }
        float disc = (1.0 - smoothstep(R - px * 2.0, R, d)) * stripes;
        col = mix(col, sun, disc);
        col += mix(magenta, orange, 0.5) * exp(-max(d - R, 0.0) * 5.0) * 0.35;

        // ---- wireframe mountains, left and right, the middle left open
        for (int m = 0; m < 2; m++) {
            float fm = float(m);
            float side = smoothstep(0.25, 1.1, abs(p.x));
            float top = horizon + (0.08 + 0.32 * ridge(p.x * 0.9 + fm * 7.0, fm * 3.0)) * side * (1.0 - fm * 0.35);
            if (p.y < top) {
                float3 rock = mix(float3(0.06, 0.02, 0.11), float3(0.10, 0.03, 0.16), fm);
                col = rock;
                // contour lines running across the slope
                float lines = hair(abs(fract((top - p.y) * 22.0) - 0.5) - 0.46, 0.04);
                col += (fm == 0.0 ? cyan : magenta) * lines * 0.10 * smoothstep(horizon, top, p.y);
            }
            col += (fm == 0.0 ? cyan : magenta) * hair(abs(p.y - top), px * 1.6) * side * 0.9;
            col += (fm == 0.0 ? cyan : magenta) * exp(-abs(p.y - top) * 60.0) * side * 0.15;
        }

        // ---- the skyline, standing in front of the sun
        float bw = 0.055;
        float bx = floor(p.x / bw);
        float bh = hash21(float2(bx, 2.0));
        float centre = exp(-pow(p.x / 1.4, 2.0));
        float height = (0.05 + 0.30 * bh * bh) * (0.35 + 0.9 * centre);
        float gap = step(0.08, fract(p.x / bw)) * step(fract(p.x / bw), 0.94);
        if (p.y < horizon + height && gap > 0.5) {
            col = float3(0.035, 0.012, 0.07);
            // windows: a grid, some lit, a few flickering
            float2 w = float2(fract(p.x / bw) * 6.0, (p.y - horizon) * 90.0);
            float2 wi = floor(w);
            float2 wf = fract(w);
            float lit = step(0.62, hash21(wi + bx * 13.0 + floor(t * 0.15 + hash21(wi + bx) * 4.0) * 0.0));
            float flick = step(0.15, fract(sin(dot(wi, float2(12.9, 78.2)) + bx) * 43758.5 + t * 0.7));
            float pane = step(0.25, wf.x) * step(wf.x, 0.75) * step(0.3, wf.y) * step(wf.y, 0.7);
            float3 wc = hash21(wi + bx) > 0.7 ? cyan : yellow;
            col += wc * pane * lit * mix(0.35, 0.8, flick) * step(wi.x, 4.5) * step(0.5, wi.x);
            // a neon sign on some roofs
            float top = horizon + height;
            if (bh > 0.6) {
                float sign = hair(abs(p.y - (top - 0.012)), px * 2.5) * step(0.2, fract(p.x / bw)) * step(fract(p.x / bw), 0.8);
                float on = step(0.08, fract(t * 0.5 + bh * 7.0));
                col += (hash21(float2(bx, 9.0)) > 0.5 ? magenta : cyan) * sign * on * 1.3;
            }
        }
        // the skyline's rim catching the sunset
        col += magenta * hair(abs(p.y - (horizon + height)), px * 1.5) * gap * 0.35;
    } else {
        // ---- the floor: a magenta grid rolling toward you
        float below = horizon - p.y;
        float z = 0.3 / below;
        float2 g = float2(p.x * z * 1.1, z + t * 0.9);
        float2 fw = fwidth(g) * 1.4;
        float2 f = abs(fract(g) - 0.5);
        float lines = max(hair(0.5 - f.x, fw.x), hair(0.5 - f.y, fw.y));
        float fade = exp(-z * 0.12);
        col = float3(0.05, 0.01, 0.09) * (1.0 - smoothstep(0.0, 1.0, below));
        col += magenta * lines * fade * 0.85;
        col += magenta * lines * fade * exp(-below * 4.0) * 0.6;
        // the sun reflected on the floor, broken up by the grid
        float streak = exp(-pow(p.x / 0.35, 2.0)) * exp(-below * 2.2);
        col += mix(yellow, magenta, smoothstep(0.0, 0.4, below)) * streak * 0.35 * (0.7 + 0.3 * sin(below * 120.0 - t * 4.0));
    }
    // the horizon's glow
    col += mix(magenta, cyan, 0.5 + 0.5 * sin(p.x * 0.5)) * exp(-abs(p.y - horizon) * 45.0) * 0.45;

    // ---- the tape: scanlines, grain, the tracking band's static, vignette
    col *= 0.88 + 0.12 * sin(uv.y * res.y * 1.5);
    col += (hash21(uv * res + fract(t * 7.0)) - 0.5) * 0.035;
    col += inBand * (hash21(float2(uv.x * 400.0, t * 60.0)) * 0.25);
    col = float3(col.r + inBand * 0.08, col.g, col.b + inBand * 0.06);
    float2 vq = (in.uv - 0.5) * 2.0;
    col *= 1.0 - 0.35 * dot(vq, vq) * 0.5;
    return float4(max(col, 0.0), 1.0);
}
"#,
    ),
    (
        "eclipse",
        r#"
// Eclipse — totality. A black moon hangs over the sun high in a midnight sky,
// the corona blazing round it in white and gold, its streamers fraying and
// slowly turning, a rose prominence licking at the rim. All round the horizon
// the sky glows like a sunset that has nowhere to set. The stars are out:
// a band of the galaxy behind, a constellation drawn in silver hairlines,
// thin orbits with planets creeping round the eclipse, and now and then a
// shooting star — or, for a breath, a bead of sunlight on the moon's edge.
static float hash21(float2 p) {
    p = fract(p * float2(234.34, 435.345));
    p += dot(p, p + 34.23);
    return fract(p.x * p.y);
}
static float vnoise(float2 p) {
    float2 i = floor(p);
    float2 f = fract(p);
    float2 w = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + float2(1, 0)), w.x),
               mix(hash21(i + float2(0, 1)), hash21(i + float2(1, 1)), w.x), w.y);
}
static float fbm(float2 p) {
    float v = 0.0, a = 0.5;
    for (int i = 0; i < 5; i++) { v += a * vnoise(p); p = p * 2.03 + 17.0; a *= 0.5; }
    return v;
}
static float sdSeg(float2 p, float2 a, float2 b) {
    float2 pa = p - a, ba = b - a;
    float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}
static float hair(float d, float w) { return 1.0 - smoothstep(0.0, w, d); }

// One layer of stars: a cell grid, a star in some cells, twinkling.
static float3 stars(float2 p, float scale, float density, float t, float3 tint) {
    float2 sp = p * scale;
    float2 id = floor(sp);
    float h = hash21(id);
    if (h < density) return float3(0.0);
    float2 off = float2(hash21(id + 1.3), hash21(id + 4.1)) - 0.5;
    float d = length(fract(sp) - 0.5 - off * 0.7);
    float tw = 0.55 + 0.45 * sin(t * (0.8 + h * 3.0) + h * 90.0);
    float size = 14.0 + 30.0 * hash21(id + 9.7);
    float3 c = mix(tint, float3(1.0, 0.9, 0.75), step(0.985, hash21(id + 2.2)));
    return c * exp(-d * size) * tw;
}

fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 res = u.resolution;
    float aspect = res.x / max(res.y, 1.0);
    float t = u.time;
    float2 uv = in.uv;
    float2 p = (uv - 0.5) * float2(aspect, 1.0) * 2.0;
    float px = 2.0 / max(res.y, 1.0);

    float3 gold   = float3(0.95, 0.78, 0.42);
    float3 silver = float3(0.78, 0.84, 0.98);
    float3 ember  = float3(1.00, 0.55, 0.28);
    float3 rose   = float3(1.00, 0.40, 0.55);
    float3 white  = float3(1.00, 0.98, 0.94);

    // ---- the sky at totality: midnight above, a ring of sunset at the horizon
    float3 col = mix(float3(0.012, 0.018, 0.045), float3(0.004, 0.006, 0.018), smoothstep(-0.6, 1.0, p.y));
    float horizon = exp(-max(p.y + 0.95, 0.0) * 3.2);
    col += mix(ember * 0.55, gold * 0.35, smoothstep(-1.0, -0.6, p.y)) * horizon * 0.55;
    col += float3(0.05, 0.07, 0.16) * exp(-max(p.y + 0.6, 0.0) * 2.0) * 0.6;

    // ---- the galaxy: a faint band running corner to corner
    {
        float2 g = float2(p.x * 0.8 + p.y * 0.6, -p.x * 0.6 + p.y * 0.8);
        float band = exp(-pow((g.y + 0.15 + 0.12 * sin(g.x * 1.3)) / 0.32, 2.0));
        float dust = fbm(g * 2.2 + float2(t * 0.004, 0.0));
        float lanes = smoothstep(0.35, 0.75, fbm(g * 4.5 + 3.0));
        col += mix(silver * 0.05, float3(0.10, 0.08, 0.14), dust) * band * (0.5 + 0.8 * dust) * (1.0 - 0.6 * lanes);
        col += stars(p, 90.0, 0.80, t, silver) * band * 0.5;
    }
    col += stars(p, 26.0, 0.955, t, silver) * 0.9;
    col += stars(p + 3.7, 48.0, 0.965, t, silver) * 0.55;

    // ---- the eclipse
    float2 E = float2(aspect * 0.42, 0.38);
    float R = 0.19;
    float2 q = p - E;
    float r = length(q);
    float a = atan2(q.y, q.x);

    // the corona: soft light falling away, frayed into turning streamers
    float outside = max(r - R, 0.0);
    // noise sampled round the circle, not along the angle, so there's no seam
    float2 dir = q / max(r, 1e-4);
    float2 turn = float2(cos(t * 0.006), sin(t * 0.006));
    float2 dr = float2(dir.x * turn.x - dir.y * turn.y, dir.x * turn.y + dir.y * turn.x);
    float streak = fbm(dr * 2.2 + float2(outside * 3.0 - t * 0.03, 0.0));
    float rays = pow(fbm(dr * 7.0 + 1.7), 2.0);
    float corona = exp(-outside * 9.0) * 0.9 + exp(-outside * 3.2) * (0.25 + 0.9 * rays) * (0.6 + 0.6 * streak);
    // long polar plumes, the way a real corona is stretched
    corona += exp(-outside * 2.0) * pow(abs(cos(a - 0.35)), 18.0) * 0.35 * (0.6 + 0.6 * streak);
    float breathe = 0.92 + 0.08 * sin(t * 0.35);
    float3 cor = mix(white, gold, smoothstep(0.0, 0.08, outside));
    cor = mix(cor, ember, smoothstep(0.08, 0.5, outside));
    col += cor * corona * breathe * step(R, r);

    // the chromosphere: a thin hot rim, and a rose prominence licking at it
    col += white * hair(abs(r - R), px * 2.5) * 0.9 * step(R - px * 2.0, r);
    {
        float pa = 2.4;
        float d = abs(a - pa);
        float tongue = exp(-pow(d / 0.09, 2.0)) * exp(-outside * 22.0);
        tongue *= 0.6 + 0.4 * vnoise(float2(outside * 60.0 - t * 0.4, 0.0));
        col += rose * tongue * 1.4;
    }

    // the moon's face: night, with the faintest earthshine
    if (r < R) {
        float shine = fbm(q * 9.0 + 2.0);
        col = float3(0.006, 0.008, 0.016) + silver * 0.018 * shine * (1.0 - r / R);
    }

    // Baily's beads: every so often a bead of sunlight flashes on the edge
    {
        float cycle = 37.0;
        float k = floor(t / cycle);
        float local = t - k * cycle;
        float flash = exp(-pow((local - 3.0) / 0.5, 2.0));
        float ba = 6.2831853 * hash21(float2(k, 3.3));
        float2 bp = E + R * float2(cos(ba), sin(ba));
        float bd = length(p - bp);
        col += white * flash * (exp(-bd * 90.0) * 2.5 + exp(-bd * 14.0) * 0.4);
        // the diamond's spikes
        float2 bq = p - bp;
        col += white * flash * smoothstep(R * 0.99, R * 1.06, r)
             * (exp(-abs(bq.y) * 400.0) * exp(-abs(bq.x) * 9.0)
              + exp(-abs(bq.x) * 400.0) * exp(-abs(bq.y) * 9.0)) * 0.6;
    }

    // ---- orbits round the eclipse, planets creeping along them
    for (int o = 0; o < 3; o++) {
        float fo = float(o);
        float2 radii = float2(0.42 + fo * 0.2, 0.16 + fo * 0.07);
        float tilt = -0.32;
        float2 oq = float2(q.x * cos(tilt) + q.y * sin(tilt), -q.x * sin(tilt) + q.y * cos(tilt));
        float e = length(oq / radii) - 1.0;
        float ring = hair(abs(e) * min(radii.x, radii.y), px * 1.2);
        // behind the moon, the orbit is hidden
        ring *= step(R * 1.02, r);
        col += silver * ring * (0.16 - fo * 0.03);
        float ang = t * (0.05 - fo * 0.012) + fo * 2.2;
        float2 pl = float2(cos(ang) * radii.x, sin(ang) * radii.y);
        float pd = length(oq - pl);
        float3 pc = (o == 1) ? gold : silver;
        col += pc * (exp(-pd * 220.0) * 1.2 + exp(-pd * 30.0) * 0.08) * step(R, length(float2(
            pl.x * cos(-tilt) + pl.y * sin(-tilt), -pl.x * sin(-tilt) + pl.y * cos(-tilt))));
    }

    // ---- a constellation, drawn in silver hairlines low on the left
    {
        float2 C = float2(-aspect * 0.55, -0.25);
        float2 s0 = C + float2(-0.25, 0.10), s1 = C + float2(-0.05, 0.22), s2 = C + float2(0.12, 0.12);
        float2 s3 = C + float2(0.30, 0.20), s4 = C + float2(0.22, -0.05), s5 = C + float2(0.40, -0.12);
        float d = sdSeg(p, s0, s1);
        d = min(d, sdSeg(p, s1, s2));
        d = min(d, sdSeg(p, s2, s3));
        d = min(d, sdSeg(p, s2, s4));
        d = min(d, sdSeg(p, s4, s5));
        col += silver * hair(d, px * 1.1) * 0.13;
        float2 ss[6] = { s0, s1, s2, s3, s4, s5 };
        for (int i = 0; i < 6; i++) {
            float sd = length(p - ss[i]);
            float tw = 0.75 + 0.25 * sin(t * 1.3 + float(i) * 1.7);
            col += mix(silver, white, 0.5) * (exp(-sd * 260.0) * 1.4 + exp(-sd * 40.0) * 0.08) * tw;
        }
    }

    // ---- a shooting star now and then
    {
        float cycle = 23.0;
        float k = floor(t / cycle);
        float local = t - k * cycle;
        float life = clamp(local / 0.9, 0.0, 1.0);
        float on = step(local, 0.9) * step(0.35, hash21(float2(k, 9.1)));
        float2 from = float2((hash21(float2(k, 1.0)) - 0.5) * aspect * 1.6, 0.55 + 0.35 * hash21(float2(k, 2.0)));
        float2 dir = normalize(float2(0.9, -0.38));
        float2 head = from + dir * life * 0.9;
        float2 tail = head - dir * 0.28;
        float d = sdSeg(p, tail, head);
        float along = clamp(dot(p - tail, dir) / 0.28, 0.0, 1.0);
        col += mix(silver, white, along) * hair(d, px * 2.0) * along * on * sin(3.14159 * life) * 1.4;
    }

    // ---- grain and vignette
    // grain from a sine hash: the cell hash runs in columns at full resolution
    col += (fract(sin(dot(floor(uv * res) + fract(t * 3.0) * 97.0, float2(12.9898, 78.233))) * 43758.5453) - 0.5) * 0.012;
    float2 vq = (in.uv - 0.5) * 2.0;
    col *= 1.0 - 0.3 * dot(vq, vq) * 0.5;
    return float4(max(col, 0.0), 1.0);
}
"#,
    ),
    (
        "abyss",
        r#"
// Abyss — the deep. Far above, the surface is a dim shimmer, and shafts of
// light sway down through the water and give out long before the bottom.
// Caustics ripple over the sea floor. Jellyfish drift up through the dark,
// their bells pulsing teal and violet, tentacles trailing; plankton hangs in
// the water at every depth, lit and fading; columns of bubbles rise from vents
// in the floor.
static float hash21(float2 p) {
    p = fract(p * float2(234.34, 435.345));
    p += dot(p, p + 34.23);
    return fract(p.x * p.y);
}
static float vnoise(float2 p) {
    float2 i = floor(p);
    float2 f = fract(p);
    float2 w = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + float2(1, 0)), w.x),
               mix(hash21(i + float2(0, 1)), hash21(i + float2(1, 1)), w.x), w.y);
}
static float fbm(float2 p) {
    float v = 0.0, a = 0.5;
    for (int i = 0; i < 4; i++) { v += a * vnoise(p); p = p * 2.03 + 17.0; a *= 0.5; }
    return v;
}
static float hair(float d, float w) { return 1.0 - smoothstep(0.0, w, d); }

// The classic water-caustic web: a few rounds of warping, summed.
static float caustic(float2 uv, float t) {
    float2 p = fmod(uv * 6.2831853, 6.2831853) - 250.0;
    float2 i = p;
    float c = 1.0;
    float inten = 0.005;
    for (int n = 0; n < 4; n++) {
        float tn = t * (1.0 - (3.5 / float(n + 1)));
        i = p + float2(cos(tn - i.x) + sin(tn + i.y), sin(tn - i.y) + cos(tn + i.x));
        c += 1.0 / length(float2(p.x / (sin(i.x + tn) / inten), p.y / (cos(i.y + tn) / inten)));
    }
    c /= 4.0;
    c = 1.17 - pow(c, 1.4);
    return pow(abs(c), 8.0);
}

// Plankton: one layer of drifting specks in a cell grid.
static float3 specks(float2 p, float scale, float t, float speed, float3 a, float3 b) {
    float2 sp = p * scale + float2(0.0, -t * speed);
    float2 id = floor(sp);
    float h = hash21(id);
    if (h < 0.86) return float3(0.0);
    float2 off = float2(hash21(id + 1.3), hash21(id + 4.1)) - 0.5;
    off.x += 0.25 * sin(t * 0.6 + h * 40.0);
    float d = length(fract(sp) - 0.5 - off * 0.6);
    float glow = 0.5 + 0.5 * sin(t * (0.6 + h * 2.0) + h * 70.0);
    return mix(a, b, hash21(id + 7.7)) * exp(-d * 22.0) * glow;
}

fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 res = u.resolution;
    float aspect = res.x / max(res.y, 1.0);
    float t = u.time;
    float2 uv = in.uv;
    float2 p = (uv - 0.5) * float2(aspect, 1.0) * 2.0;
    float px = 2.0 / max(res.y, 1.0);

    float3 teal     = float3(0.18, 0.90, 0.84);
    float3 jelly    = float3(0.55, 0.48, 1.00);
    float3 plankton = float3(0.36, 1.00, 0.72);
    float3 water    = float3(0.00, 0.20, 0.30);

    // ---- the water: lit from far above, black below
    float depth = 1.0 - uv.y;
    float3 col = mix(float3(0.010, 0.085, 0.13), float3(0.002, 0.010, 0.020), smoothstep(0.0, 0.85, depth));
    // the surface: a faint shimmer at the very top
    float surface = smoothstep(0.82, 1.0, uv.y);
    col += water * 0.5 * surface * (0.6 + 0.4 * vnoise(float2(p.x * 6.0 + t * 0.3, t * 0.2)));

    // ---- shafts of light swaying down from the surface
    {
        float shafts = 0.0;
        for (int i = 0; i < 6; i++) {
            float fi = float(i);
            float x0 = (hash21(float2(fi, 1.0)) - 0.5) * aspect * 2.2;
            float lean = 0.22 + 0.08 * sin(t * 0.12 + fi);
            float x = p.x - x0 - (1.0 - p.y) * lean;
            float w = 0.05 + 0.07 * hash21(float2(fi, 2.0));
            float flick = 0.6 + 0.4 * sin(t * (0.3 + 0.1 * fi) + fi * 3.0);
            shafts += exp(-pow(x / w, 2.0)) * flick;
        }
        float fall = smoothstep(1.0, 0.25, depth) * (0.75 + 0.25 * vnoise(float2(p.x * 3.0, p.y * 2.0 - t * 0.2)));
        col += mix(teal, float3(0.6, 0.95, 1.0), 0.3) * shafts * fall * 0.07;
    }

    // ---- the sea floor, its caustics, and a mist above it
    float floorY = -0.78 + 0.05 * sin(p.x * 1.7) + 0.03 * sin(p.x * 4.3 + 1.0);
    if (p.y < floorY) {
        float below = floorY - p.y;
        float3 sand = mix(float3(0.010, 0.040, 0.050), float3(0.004, 0.012, 0.018), smoothstep(0.0, 0.25, below));
        // the floor recedes away from us, so the web is squashed toward the back
        float2 fq = float2(p.x * 0.55, (0.3 - below) * 1.6);
        float c = caustic(fq, t * 0.3);
        col = sand + mix(teal, float3(0.7, 1.0, 1.0), 0.3) * min(c, 1.5) * 0.55 * smoothstep(0.32, 0.0, below);
    }
    col += teal * 0.04 * exp(-max(p.y - floorY, 0.0) * 6.0);

    // ---- vents: columns of bubbles rising from the floor
    for (int v = 0; v < 3; v++) {
        float fv = float(v);
        float vx = (hash21(float2(fv, 11.0)) - 0.5) * aspect * 1.7;
        for (int b = 0; b < 9; b++) {
            float fb = float(b);
            float h = hash21(float2(fv, fb + 20.0));
            float rise = fract(t * (0.05 + 0.03 * h) + h);
            float by = floorY + rise * 2.0;
            float bx = vx + 0.03 * sin(t * 2.0 + fb * 1.7 + fv) * rise;
            float br = 0.006 + 0.01 * hash21(float2(fb, fv + 5.0)) * (0.6 + rise);
            float d = length(p - float2(bx, by)) - br;
            float fade = (1.0 - rise) * smoothstep(0.0, 0.05, rise);
            col += mix(teal, float3(0.8, 1.0, 1.0), 0.5) * (hair(abs(d), px * 1.5) * 0.7 + exp(-abs(d) * 120.0) * 0.12) * fade;
        }
    }

    // ---- plankton at every depth
    col += specks(p, 9.0, t, 0.05, teal, plankton) * 0.55;
    col += specks(p + 2.3, 16.0, t, 0.08, teal, jelly) * 0.35;
    col += specks(p + 5.1, 28.0, t, 0.12, plankton, teal) * 0.25;

    // ---- jellyfish drifting up through the dark
    for (int j = 0; j < 3; j++) {
        float fj = float(j);
        float period = 80.0 + fj * 23.0;
        float rise = fract(t / period + fj * 0.37);
        float2 c = float2((hash21(float2(fj, 3.0)) - 0.5) * aspect * 1.4 + 0.15 * sin(t * 0.07 + fj * 2.0),
                          -1.5 + rise * 3.0);
        float size = 0.07 + 0.05 * hash21(float2(fj, 4.0));
        // the bell contracts and relaxes
        float beat = 0.5 + 0.5 * sin(t * 1.6 + fj * 2.1);
        float2 q = (p - c) / size;
        q.x *= 1.0 + 0.12 * beat;
        q.y *= 1.0 - 0.1 * beat;
        float3 tint = (j == 1) ? jelly : teal;
        // the bell: a dome, bright at its rim, its margin a scalloped skirt
        // rippling with the beat
        float skirt = -0.1 - 0.08 * (0.5 + 0.5 * sin(q.x * 11.0 + t * 3.0));
        float margin = smoothstep(skirt - 0.06, skirt + 0.04, q.y);
        float dome = length(float2(q.x, max(q.y, 0.0) * 1.3));
        float bell = smoothstep(1.0, 0.82, dome) * margin;
        float rim = exp(-abs(dome - 1.0) * 10.0) * margin;
        float edge = exp(-abs(q.y - skirt) * 18.0) * smoothstep(1.05, 0.7, abs(q.x)) * 0.6;
        float inner = exp(-length(q - float2(0.0, 0.3)) * 2.6);
        // the frilled arms under the bell, brighter than the tentacles
        float frill = 0.0;
        if (q.y < 0.0 && q.y > -1.6) {
            float wob = 0.12 * sin(q.y * 6.0 + t * 2.0);
            frill = exp(-abs(q.x - wob) * 9.0) * smoothstep(-1.6, -0.1, q.y) * 0.5;
        }
        float3 light = tint * (bell * 0.28 + rim * 0.7 + edge + inner * 0.45 + frill) * (0.7 + 0.3 * beat);
        // tentacles trailing below, swaying
        float tent = 0.0;
        if (q.y < 0.0 && q.y > -4.5) {
            for (int k = 0; k < 4; k++) {
                float fk = float(k);
                float x0 = -0.55 + fk * 0.37;
                float sway = 0.18 * sin(q.y * 1.8 + t * 1.3 + fk) * (-q.y) * 0.35;
                float d = abs(q.x - x0 - sway);
                tent += exp(-d * 16.0) * smoothstep(-4.5, -0.2, q.y) * smoothstep(0.0, -0.3, q.y);
            }
        }
        light += tint * tent * 0.16;
        // a soft glow round the whole creature
        light += tint * exp(-length(q) * 0.8) * 0.08;
        float vis = smoothstep(0.0, 0.08, rise) * smoothstep(1.0, 0.9, rise);
        col += light * vis;
    }

    // ---- particulate haze, grain, vignette
    col += teal * 0.015 * fbm(p * 3.0 + float2(0.0, -t * 0.03));
    // grain from a sine hash: the cell hash runs in columns at full resolution
    col += (fract(sin(dot(floor(uv * res) + fract(t * 3.0) * 97.0, float2(12.9898, 78.233))) * 43758.5453) - 0.5) * 0.01;
    float2 vq = (in.uv - 0.5) * 2.0;
    col *= 1.0 - 0.4 * dot(vq, vq) * 0.5;
    return float4(max(col, 0.0), 1.0);
}
"#,
    ),
];

pub fn builtin_source(name: &str) -> Option<&'static str> {
    BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

pub fn builtin_names() -> Vec<&'static str> {
    BUILTINS.iter().map(|(n, _)| *n).collect()
}

// ---- pipeline -------------------------------------------------------------

fn build_gpu(source: &str) -> Result<Gpu, String> {
    unsafe {
        let device = MTLCreateSystemDefaultDevice().ok_or("no Metal device on this Mac")?;
        let queue = device.newCommandQueue().ok_or("could not create a command queue")?;

        let full = format!("{PREAMBLE}\n{source}");
        let ns_src = NSString::from_str(&full);
        let library = device
            .newLibraryWithSource_options_error(&ns_src, None)
            .map_err(|e| format!("shader compile failed:\n{}", e.localizedDescription()))?;

        let vname = NSString::from_str("v_main");
        let fname = NSString::from_str("f_main");
        let vfn = library
            .newFunctionWithName(&vname)
            .ok_or("shader is missing the vertex function v_main")?;
        let ffn = library
            .newFunctionWithName(&fname)
            .ok_or("shader must define: fragment float4 f_main(...)")?;

        let desc = MTLRenderPipelineDescriptor::new();
        desc.setVertexFunction(Some(&vfn));
        desc.setFragmentFunction(Some(&ffn));
        {
            let attachments = desc.colorAttachments();
            let a0 = attachments.objectAtIndexedSubscript(0);
            a0.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        }
        let pipeline = device
            .newRenderPipelineStateWithDescriptor_error(&desc)
            .map_err(|e| format!("pipeline build failed:\n{}", e.localizedDescription()))?;

        Ok(Gpu {
            device,
            queue,
            pipeline,
        })
    }
}

unsafe fn encode_frame(
    gpu: &Gpu,
    texture: &ProtocolObject<dyn MTLTexture>,
    uniforms: &Uniforms,
) -> Result<Retained<ProtocolObject<dyn objc2_metal::MTLCommandBuffer>>, String> {
    let pass = MTLRenderPassDescriptor::renderPassDescriptor();
    {
        let attachments = pass.colorAttachments();
        let a0 = attachments.objectAtIndexedSubscript(0);
        a0.setTexture(Some(texture));
        a0.setLoadAction(MTLLoadAction::Clear);
        a0.setStoreAction(MTLStoreAction::Store);
        a0.setClearColor(MTLClearColor {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        });
    }

    let cb = gpu.queue.commandBuffer().ok_or("no command buffer")?;
    let enc = cb
        .renderCommandEncoderWithDescriptor(&pass)
        .ok_or("no render encoder")?;
    enc.setRenderPipelineState(&gpu.pipeline);
    enc.setFragmentBytes_length_atIndex(
        std::ptr::NonNull::new_unchecked(uniforms as *const Uniforms as *mut c_void),
        std::mem::size_of::<Uniforms>(),
        0,
    );
    enc.drawPrimitives_vertexStart_vertexCount(MTLPrimitiveType::Triangle, 0, 3);
    enc.endEncoding();
    Ok(cb)
}

// ---- offscreen check ------------------------------------------------------

/// Render exactly one frame offscreen and report what came back, so a shader
/// can be verified without looking at the screen.
pub fn check(source: &str, width: u32, height: u32) -> Result<String, String> {
    let (msg, _buf) = check_with_pixels(source, width, height, 1.0)?;
    Ok(msg)
}

/// Same as `check`, plus the raw BGRA pixels so a caller can look at the frame.
pub fn check_with_pixels(
    source: &str,
    width: u32,
    height: u32,
    time: f32,
) -> Result<(String, Vec<u8>), String> {
    let gpu = build_gpu(source)?;
    unsafe {
        let desc = objc2_metal::MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            MTLPixelFormat::BGRA8Unorm,
            width as usize,
            height as usize,
            false,
        );
        desc.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        desc.setStorageMode(MTLStorageMode::Shared);
        let texture = gpu
            .device
            .newTextureWithDescriptor(&desc)
            .ok_or("could not allocate the check texture")?;

        let uniforms = Uniforms {
            resolution: [width as f32, height as f32],
            time,
            frame: 1.0,
            mouse: [0.0, 0.0],
        };
        let cb = encode_frame(&gpu, &texture, &uniforms)?;
        cb.commit();
        cb.waitUntilCompleted();

        let row_bytes = width as usize * 4;
        let mut buf = vec![0u8; row_bytes * height as usize];
        let region = objc2_metal::MTLRegion {
            origin: objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
            size: objc2_metal::MTLSize {
                width: width as usize,
                height: height as usize,
                depth: 1,
            },
        };
        texture.getBytes_bytesPerRow_fromRegion_mipmapLevel(
            std::ptr::NonNull::new_unchecked(buf.as_mut_ptr() as *mut c_void),
            row_bytes,
            region,
            0,
        );

        let mut sum = [0u64; 3];
        let mut min = [255u8; 3];
        let mut max = [0u8; 3];
        for px in buf.chunks_exact(4) {
            for c in 0..3 {
                let v = px[c];
                sum[c] += v as u64;
                min[c] = min[c].min(v);
                max[c] = max[c].max(v);
            }
        }
        let n = (width * height) as u64;
        let mean = [
            sum[0] / n.max(1),
            sum[1] / n.max(1),
            sum[2] / n.max(1),
        ];
        Ok((format!(
            "ok  {}x{}  mean rgb({},{},{})  min({},{},{})  max({},{},{})",
            width,
            height,
            mean[2],
            mean[1],
            mean[0],
            min[2],
            min[1],
            min[0],
            max[2],
            max[1],
            max[0]
        ), buf))
    }
}

/// Coarse luminance map, for looking at a shader without a screenshot.
pub fn ascii_frame(pixels: &[u8], width: u32, height: u32, cols: u32, rows: u32) -> String {
    const RAMP: &[u8] = b" .:-=+*#%@";
    let mut out = String::new();
    for ry in 0..rows {
        for rx in 0..cols {
            let x0 = rx * width / cols;
            let x1 = ((rx + 1) * width / cols).max(x0 + 1);
            let y0 = ry * height / rows;
            let y1 = ((ry + 1) * height / rows).max(y0 + 1);
            let mut sum = 0u32;
            let mut n = 0u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = ((y * width + x) * 4) as usize;
                    let lum = (pixels[i + 2] as u32 * 30
                        + pixels[i + 1] as u32 * 59
                        + pixels[i] as u32 * 11)
                        / 100;
                    sum += lum;
                    n += 1;
                }
            }
            let lum = (sum / n.max(1)) as usize * (RAMP.len() - 1) / 255;
            out.push(RAMP[lum] as char);
        }
        out.push('\n');
    }
    out
}

// ---- live wallpaper -------------------------------------------------------

struct LayerRef(Retained<CAMetalLayer>);
// The layer is created on the main thread and then only touched by its own
// render thread, which is exactly the ownership CAMetalLayer expects.
unsafe impl Send for LayerRef {}

impl LayerRef {
    // A method call, so the closure captures the whole LayerRef (which is Send)
    // instead of its inner field (which is not).
    fn get(&self) -> &CAMetalLayer {
        &self.0
    }
}

struct ScreenWindow {
    window: Retained<NSWindow>,
    layer: LayerRef,
    size: [f64; 2],
}

unsafe fn create_window(mtm: MainThreadMarker, frame: NSRect, gpu: &Gpu) -> ScreenWindow {
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

    let view: Retained<NSView> = msg_send![NSView::alloc(mtm), initWithFrame: frame];
    view.setWantsLayer(true);
    view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );

    let layer = CAMetalLayer::new();
    layer.setDevice(Some(&gpu.device));
    layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
    layer.setFramebufferOnly(true);
    layer.setOpaque(true);

    // Drawn at one pixel per point and scaled up by the compositor: four times
    // fewer pixels than Retina, which these soft, glowing shaders barely show,
    // and it is what keeps a full-screen shader from saturating the GPU the
    // desktop needs. LWP_SHADER_SCALE=2 for full Retina.
    let scale = std::env::var("LWP_SHADER_SCALE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| v.clamp(0.25, 2.0))
        .unwrap_or(1.0);
    let drawable = objc2_core_foundation::CGSize {
        width: frame.size.width * scale,
        height: frame.size.height * scale,
    };
    layer.setDrawableSize(drawable);
    let bounds = view.bounds();
    let _: () = msg_send![&layer, setFrame: bounds];

    view.setLayer(Some(&layer));
    win.setContentView(Some(&view));
    win.makeKeyAndOrderFront(None);

    ScreenWindow {
        window: win,
        layer: LayerRef(layer),
        size: [drawable.width, drawable.height],
    }
}

fn spawn_renderer(gpu: Arc<Gpu>, layer: LayerRef, size: [f64; 2]) {
    thread::spawn(move || {
        let layer = layer.get();
        let start = Instant::now();
        let mut frame: u32 = 0;
        let res = [size[0] as f32, size[1] as f32];
        // A wallpaper needs no more than 30 frames a second; drawing at the
        // display's 120 just burns GPU time the windows on top of it want.
        let fps = std::env::var("LWP_SHADER_FPS")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .map(|v| v.clamp(5.0, 120.0))
            .unwrap_or(30.0);
        let period = std::time::Duration::from_secs_f64(1.0 / fps);
        let mut next = Instant::now();

        while !SHOULD_QUIT.load(Ordering::SeqCst) {
            let now = Instant::now();
            if now < next {
                thread::sleep(next - now);
            }
            next = (next + period).max(Instant::now());
            unsafe {
                let Some(drawable) = layer.nextDrawable() else {
                    thread::sleep(std::time::Duration::from_millis(8));
                    continue;
                };
                let texture = drawable.texture();
                let uniforms = Uniforms {
                    resolution: res,
                    time: start.elapsed().as_secs_f32(),
                    frame: frame as f32,
                    mouse: [0.0, 0.0],
                };
                match encode_frame(&gpu, &texture, &uniforms) {
                    Ok(cb) => {
                        let _: () = msg_send![&*cb, presentDrawable: &*drawable];
                        cb.commit();
                    }
                    Err(_) => break,
                }
            }
            frame = frame.wrapping_add(1);
        }
    });
}

pub fn run(source: &str, _hide_icons: bool) -> Result<(), String> {
    unsafe {
        libc::signal(libc::SIGTERM, sigterm_handler as *const () as usize);
    }

    let gpu = Arc::new(build_gpu(source)?);

    let mtm = MainThreadMarker::new().ok_or("must run on the main thread")?;
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app.finishLaunching();

    let screens = NSScreen::screens(mtm);
    let mut windows: Vec<Arc<ScreenWindow>> = screens
        .iter()
        .map(|s| Arc::new(unsafe { create_window(mtm, s.frame(), &gpu) }))
        .collect();

    for w in &windows {
        spawn_renderer(gpu.clone(), LayerRef(w.layer.0.clone()), w.size);
    }

    let run_loop = NSRunLoop::currentRunLoop();
    while !SHOULD_QUIT.load(Ordering::SeqCst) {
        let limit = NSDate::dateWithTimeIntervalSinceNow(0.1);
        run_loop.runUntilDate(&limit);
    }

    for w in windows.drain(..) {
        unsafe {
            w.window.setReleasedWhenClosed(false);
            w.window.orderOut(None);
            w.window.close();
        }
    }
    Ok(())
}

extern "C" fn sigterm_handler(_: i32) {
    SHOULD_QUIT.store(true, Ordering::SeqCst);
}

#[allow(dead_code)]
fn _assert_send_sync() {
    fn assert_send<T: Send>() {}
    assert_send::<Gpu>();
}

/// Write BGRA pixels (top row first) as a 32-bit BMP — enough for `sips` to turn
/// into a PNG, with no image crate.
pub fn write_bmp(path: &std::path::Path, pixels: &[u8], width: u32, height: u32) -> std::io::Result<()> {
    let row = width as usize * 4;
    let size = 54 + row * height as usize;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    // negative height: rows are stored top-down
    out.extend_from_slice(&(-(height as i32)).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    for y in 0..height as usize {
        let src = &pixels[y * row..(y + 1) * row];
        for px in src.chunks_exact(4) {
            out.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
    }
    std::fs::write(path, out)
}
