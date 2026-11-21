# lwp — live wallpapers for macOS

Plays a looping video, or draws a Metal fragment shader, as the desktop background
on every screen. No app, no dock icon: a borderless window at desktop level, one per
display, joining all spaces.

```sh
lwp set ~/Documents/livewalls/blackhole.mp4   # video
lwp set-builtin stark                          # shader
lwp status
lwp stop
lwp autostart-on                               # replay the last one at login
```

## Video wallpapers

`lwp set <file>` loops any video AVFoundation can decode, using two `AVQueuePlayer`
items so the loop has no seam. Hardware decode, ~3% CPU.

## Shader wallpapers

`lwp set-builtin <name>` or `lwp set-shader <file.metal>` replaces the player with a
`CAMetalLayer` and renders a fragment shader every frame at the display's native
resolution. Nothing is stored: the picture is computed.

Built-ins: `plasma`, `aurora`, `rings`, `stark` (an arc reactor).

A shader file only has to define the fragment stage — lwp supplies the vertex
shader, the `VOut` struct and the uniforms:

```metal
fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 p = (in.uv - 0.5) * float2(u.resolution.x / max(u.resolution.y, 1.0), 1.0);
    float r = length(p);
    return float4(float3(0.1, 0.6, 0.9) * exp(-r * 3.0), 1.0);
}
```

```metal
struct Uniforms {
    float2 resolution;   // drawable size in pixels
    float  time;         // seconds since the wallpaper started
    float  frame;        // frame counter
    float2 mouse;        // reserved
};
```

`set-shader` compiles the shader *before* it stops whatever is playing, so a broken
file cannot leave you with a black desktop.

## Checking a shader from a terminal

```sh
lwp shader-check --builtin stark
lwp shader-check ~/wall.metal --size 240x240 --ascii
```

`shader-check` renders one frame offscreen, reads the pixels back and reports
mean/min/max RGB — enough to tell "it compiles and produces light" from "it
compiled and produced black". `--ascii` prints a coarse luminance map so the
composition can be judged without a screenshot.

## State

    /tmp/lwp/pid              worker pid
    /tmp/lwp/current_video.txt   what is playing
    /tmp/lwp/lwp.log          worker stderr
    ~/.lwp/config.json        last wallpaper + kind, replayed by autostart
