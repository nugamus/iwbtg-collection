// The one background behind every screen (docs/decisions/0003-ui-toolkit.md, Backdrop).
//
// The art covers the node (never stretched), is blurred by sampling its mip chain (a 3x3 tap on
// the mip level that matches the blur radius), and is dimmed toward the mockups' #05060a. Colours
// are mixed in sRGB, as CSS does.

struct BackdropUniform {
    // x blur (0 to 1: 0 sharp, 1 is the mockups' 40 px at 1600 px wide), y dim (0 to 1),
    // z opacity (cross-fade), w highest mip level
    p: vec4<f32>,
}

@group(1) @binding(0) var<uniform> backdrop: BackdropUniform;
@group(1) @binding(1) var art: texture_2d<f32>;
@group(1) @binding(2) var art_sampler: sampler;

struct UiVertexOutput {
    @location(0) uv: vec2<f32>,
    @location(1) border_widths: vec4<f32>,
    @location(2) border_radius_x: vec4<f32>,
    @location(3) border_radius_y: vec4<f32>,
    @location(4) @interpolate(flat) size: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let img = vec2<f32>(textureDimensions(art, 0));
    // Cover: scale until both sides are filled, centred.
    let fit = max(in.size.x / img.x, in.size.y / img.y);
    let uv = (in.uv * in.size - in.size * 0.5) / fit / img + vec2<f32>(0.5);

    // The blur radius in texels of the art, then the mip level of that size.
    let sigma = backdrop.p.x * 0.025 * img.x;
    var lod = 0.0;
    if backdrop.p.x > 0.001 {
        lod = clamp(log2(max(sigma, 1.0)), 0.0, backdrop.p.w);
    }
    let texel = exp2(lod) / img;
    var sum = vec3<f32>(0.0);
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let w = (2.0 - abs(f32(dx))) * (2.0 - abs(f32(dy)));
            sum += w * textureSampleLevel(art, art_sampler, uv + vec2<f32>(f32(dx), f32(dy)) * texel, lod).rgb;
        }
    }
    let colour = sum / 16.0;
    let dimmed = mix(to_srgb(colour), vec3<f32>(5.0, 6.0, 10.0) / 255.0, backdrop.p.y);
    return vec4<f32>(to_linear(dimmed), backdrop.p.z);
}
