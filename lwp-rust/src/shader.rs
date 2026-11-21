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
// Stark — an arc reactor. Not a diagram: a machined steel housing with segment
// gaps, ten copper coils on a slowly turning rotor, a containment ring, and a
// plasma core with bloom and an anamorphic streak. Everything is procedural.
static float hash21(float2 p) {
    return fract(sin(dot(p, float2(127.1, 311.7))) * 43758.5453);
}
static float noise2(float2 p) {
    float2 i = floor(p);
    float2 f = fract(p);
    float2 w = f * f * (3.0 - 2.0 * f);
    float a = hash21(i);
    float b = hash21(i + float2(1.0, 0.0));
    float c = hash21(i + float2(0.0, 1.0));
    float d = hash21(i + float2(1.0, 1.0));
    return mix(mix(a, b, w.x), mix(c, d, w.x), w.y);
}
static float sdRoundBox(float2 p, float2 b, float r) {
    float2 q = abs(p) - b + r;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 res = u.resolution;
    float2 p = (in.uv - 0.5) * float2(res.x / max(res.y, 1.0), 1.0) * 2.0;
    float t = u.time;

    // heat shimmer: the air above the core bends what is behind it
    float2 pw = p + exp(-length(p) * 3.2) * 0.010 *
                    float2(sin(p.y * 15.0 + t * 1.9), cos(p.x * 13.0 + t * 1.4));
    p = pw;

    float r = length(p);
    float a = atan2(p.y, p.x);

    // ---- backdrop: cold blue-black, lifted slightly around the core
    float3 col = mix(float3(0.008, 0.013, 0.020),
                     float3(0.030, 0.050, 0.066),
                     1.0 - smoothstep(0.20, 1.40, r));
    col += float3(0.018, 0.045, 0.062) * noise2(p * 16.0) * 0.5;

    // ---- engineering grid on the backdrop: fine cells plus a coarse ruling,
    // fading out before it reaches the housing
    float2 gf = abs(fract(p * 3.2) - 0.5);
    float fine = smoothstep(0.465, 0.500, max(gf.x, gf.y));
    float2 gc = abs(fract(p * 0.8) - 0.5);
    float coarse = smoothstep(0.482, 0.500, max(gc.x, gc.y));
    float gridFade = 1.0 - smoothstep(0.25, 1.45, r);
    float gridPulse = 0.85 + 0.15 * sin(t * 0.5 - r * 3.0);
    col += float3(0.030, 0.105, 0.150) * fine * gridFade * 0.85 * gridPulse;
    col += float3(0.055, 0.170, 0.230) * coarse * gridFade * 0.75 * gridPulse;

    // ---- rotor: coils and inner plates turn, the housing stays put
    float rot = t * 0.045;
    float2 rp = float2(cos(rot) * p.x - sin(rot) * p.y,
                       sin(rot) * p.x + cos(rot) * p.y);
    float ra = atan2(rp.y, rp.x);

    float seg = 6.2831853 / 10.0;
    float gap = 1.0 - smoothstep(0.008, 0.030, abs(fract(a / seg + 0.5) - 0.5) * seg);

    // ---- machined steel housing: a solid plate with three raised rims
    float plate = smoothstep(0.665, 0.685, r) * (1.0 - smoothstep(1.00, 1.02, r));

    float h1 = 1.0 - smoothstep(0.0, 0.014, abs(r - 0.94));
    float h2 = 1.0 - smoothstep(0.0, 0.020, abs(r - 0.86));
    float h3 = 1.0 - smoothstep(0.0, 0.016, abs(r - 0.74));

    // brushed metal: the sheen turns with the angle, the segment gaps go dark
    float3 steel = mix(float3(0.030, 0.037, 0.047), float3(0.20, 0.235, 0.275),
                       0.5 + 0.5 * sin(a * 2.0 + 1.1));
    steel *= 0.45 + 0.55 * (1.0 - gap);

    col = mix(col, steel * 0.85, plate);
    col = mix(col, steel * 1.35, h3 * 0.9);
    col = mix(col, steel * 1.55, h2);
    col = mix(col, steel * 1.15, h1 * 0.9);

    // ---- ten copper coils, turning with the rotor
    float idx  = floor((ra + 3.14159265) / seg);
    float mid  = (idx + 0.5) * seg - 3.14159265;
    float2 dir = float2(cos(mid), sin(mid));
    float2 side = float2(-dir.y, dir.x);
    float2 local = float2(dot(rp, dir) - 0.545, dot(rp, side));

    float d    = sdRoundBox(local, float2(0.088, 0.031), 0.026);
    float coil = 1.0 - smoothstep(0.0, 0.004, d);
    float rim  = 1.0 - smoothstep(0.0, 0.012, abs(d));

    // copper, lit from the core end (local.x < 0 faces inward)
    float3 copper = mix(float3(0.46, 0.20, 0.085), float3(1.00, 0.55, 0.22),
                        1.0 - smoothstep(-0.10, 0.10, local.x));
    copper += float3(0.65, 0.34, 0.13) * rim;
    copper += float3(0.55, 0.27, 0.10) * (1.0 - smoothstep(0.0, 0.010, abs(local.y - 0.031)));

    col = mix(col, copper, coil * 0.95);

    // warm bounce: the coils light the steel around them
    col += float3(0.16, 0.065, 0.020) * exp(-abs(r - 0.545) * 16.0) * 0.9;

    // ---- containment rings
    float ring1 = 1.0 - smoothstep(0.0, 0.0045, abs(r - 0.42));
    float ring2 = 1.0 - smoothstep(0.0, 0.0028, abs(r - 0.30));
    col += float3(0.10, 0.55, 0.85) * ring1 * 0.85;
    col += float3(0.14, 0.70, 1.00) * ring2 * 0.75;

    // ---- plasma core
    float breath = 0.92 + 0.08 * sin(t * 0.85);
    float arc    = smoothstep(0.985, 1.0, noise2(float2(t * 0.7, 3.1))) * 0.35;
    float flick  = (0.90 + 0.10 * noise2(float2(a * 4.0, t * 2.2))) * breath + arc;
    float core  = 1.0 - smoothstep(0.0, 0.255, r);
    float3 hot  = float3(0.90, 0.98, 1.00);
    float3 cool = float3(0.12, 0.62, 0.95);
    float3 coreCol = mix(cool, hot, 1.0 - smoothstep(0.02, 0.20, r));
    col += coreCol * core * 1.45 * flick;

    // ---- bloom: tight hot falloff plus a wide dim halo
    col += float3(0.16, 0.62, 0.95) * (exp(-r * 4.2) * 0.85 + exp(-r * 1.35) * 0.16);

    // ---- god rays: the core seen through the gap between the coils
    float rays = pow(0.5 + 0.5 * sin(a * 18.0 + t * 0.45), 8.0) * 0.65
               + 0.35 * noise2(float2(a * 9.0, t * 0.35));
    col += float3(0.045, 0.20, 0.32) * rays * exp(-r * 2.1) * 0.50;

    // ---- energy pulse: a ring released every few seconds, fading as it goes
    float ph = fract(t * 0.16);
    float pulse = exp(-pow((r - ph * 1.15) * 16.0, 2.0)) * (1.0 - ph);
    col += float3(0.10, 0.46, 0.66) * pulse * 0.30;

    // ---- dust in the light, three parallax layers drifting up
    float dust = 0.0;
    for (int i = 0; i < 3; i++) {
        float fi = float(i);
        float2 q = p * (7.0 + fi * 3.5) + float2(fi * 13.0, t * (0.13 + fi * 0.05));
        float2 cell = floor(q);
        float2 f = fract(q) - 0.5;
        float h = hash21(cell + fi * 17.0);
        float2 off = float2(hash21(cell + 1.3), hash21(cell + 7.7)) - 0.5;
        dust += (1.0 - smoothstep(0.0, 0.05, length(f - off * 0.7))) * step(0.93, h);
    }
    col += float3(0.30, 0.62, 0.82) * dust * 0.18 * (1.0 - smoothstep(0.25, 1.30, r));

    // ---- specular sweep travelling around the housing
    float specA = fract(a / 6.2831853 - t * 0.045);
    float spec  = exp(-pow((specA - 0.5) * 14.0, 2.0));
    col += float3(0.06, 0.09, 0.11) * spec * smoothstep(0.60, 0.75, r);

    // ---- anamorphic streak, the giveaway of a very bright source
    float streak = exp(-abs(p.y) * 70.0) * exp(-abs(p.x) * 2.0);
    col += float3(0.30, 0.62, 0.85) * streak * 0.45;

    // ---- glass sheen across the face
    float sheen = smoothstep(0.72, 1.0, 1.0 - abs(dot(normalize(p + 1e-5), float2(-0.6, 0.8))));
    col += float3(0.05, 0.08, 0.10) * sheen * (1.0 - smoothstep(0.6, 1.1, r));

    // ---- grain + vignette
    col += (noise2(in.uv * res * 0.5 + t) - 0.5) * 0.006;
    col *= 1.0 - smoothstep(0.80, 1.75, r);

    return float4(col, 1.0);
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
    let (msg, _buf) = check_with_pixels(source, width, height)?;
    Ok(msg)
}

/// Same as `check`, plus the raw BGRA pixels so a caller can look at the frame.
pub fn check_with_pixels(
    source: &str,
    width: u32,
    height: u32,
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
            time: 1.0,
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

    let scale = 2.0f64;
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

        while !SHOULD_QUIT.load(Ordering::SeqCst) {
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
