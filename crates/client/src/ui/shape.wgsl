// The sheared shape behind every panel, pill, button and row (docs/decisions/0003-ui-toolkit.md).
//
// The node this material draws on is the shape's layout box grown by `margin` on each side, so
// the shear overhang, the ring and the drop shadow have room. Everything here is in logical
// pixels, measured from the box centre, y down; `s` (physical px per logical px) converts from
// the physical pixels the vertex stage hands over. Colours arrive as sRGB with straight alpha and are mixed the way CSS
// mixes them (premultiplied sRGB); the result is converted to linear for the render target.

struct ShapeUniform {
    bg: vec4<f32>,
    veil0: vec4<f32>,
    veil1: vec4<f32>,
    veil2: vec4<f32>,
    ring: vec4<f32>,
    shadow: vec4<f32>,
    band: vec4<f32>,
    // x shear (0.2), y corner radius, z physical px per logical px, w opacity
    geom: vec4<f32>,
    // x margin left and right, y margin top, z margin bottom, w ring width
    margin: vec4<f32>,
    // x veil middle stop (0 to 1), y veil direction (0 left to right, 1 top to bottom),
    // z image on (0 or 1), w band side (0 none, 1 left, 2 bottom)
    mode: vec4<f32>,
    // x shadow offset y, y shadow blur, z band width, w band fade end (share of the box width; 0 solid)
    extra: vec4<f32>,
    // Vertical fade in window pixels (physical, y down): x top edge, y top fade end,
    // z bottom fade start, w bottom edge. w <= x means no fade.
    fade: vec4<f32>,
}

@group(1) @binding(0) var<uniform> shape: ShapeUniform;
@group(1) @binding(1) var shape_image: texture_2d<f32>;
@group(1) @binding(2) var shape_sampler: sampler;

// Same locations as bevy_ui_render's UiVertexOutput.
struct UiVertexOutput {
    @location(0) uv: vec2<f32>,
    @location(1) border_widths: vec4<f32>,
    @location(2) border_radius_x: vec4<f32>,
    @location(3) border_radius_y: vec4<f32>,
    @location(4) @interpolate(flat) size: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn premul(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

// `top` over `under`, both premultiplied.
fn over(top: vec4<f32>, under: vec4<f32>) -> vec4<f32> {
    return top + under * (1.0 - top.a);
}

fn sd_round_box(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// Coverage of the area where `d < 0`, antialiased over one physical pixel.
fn cover(d: f32, aa: f32) -> f32 {
    return clamp(0.5 - d / aa, 0.0, 1.0);
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let shear = shape.geom.x;
    let s = max(shape.geom.z, 0.0001);
    let box_h = in.size.y / s - shape.margin.y - shape.margin.z;
    let local = in.uv * in.size / s;
    let box_w = in.size.x / s - 2.0 * shape.margin.x;
    let half = vec2<f32>(box_w, box_h) * 0.5;
    let p = local - vec2<f32>(shape.margin.x + half.x, shape.margin.y + half.y);
    // Undo the shear: CSS skewX(-11.31deg) moves a point right by 0.2 per pixel above the centre.
    let q = vec2<f32>(p.x + shear * p.y, p.y);
    let r = min(shape.geom.y, min(half.x, half.y));
    let d = sd_round_box(q, half, r);
    let aa = max(fwidth(d), 1.0 / s);
    let inside = cover(d, aa);

    // Fill: background, then the image (upright, covering the sheared box), then the veil.
    var fill = premul(shape.bg);
    let span = vec2<f32>(box_w + 2.0 * shear * half.y, box_h);
    let image_size = vec2<f32>(textureDimensions(shape_image));
    let fit = max(span.x / image_size.x, span.y / image_size.y);
    let image_uv = (p / fit + image_size * 0.5) / image_size;
    let texel = textureSample(shape_image, shape_sampler, image_uv);
    fill = over(texel * shape.mode.z, fill);

    var t = (q.x + half.x) / max(box_w, 1.0);
    if shape.mode.y > 0.5 {
        t = (p.y + half.y) / max(box_h, 1.0);
    }
    t = clamp(t, 0.0, 1.0);
    let mid = clamp(shape.mode.x, 0.001, 0.999);
    var veil = mix(premul(shape.veil0), premul(shape.veil1), clamp(t / mid, 0.0, 1.0));
    if t > mid {
        veil = mix(premul(shape.veil1), premul(shape.veil2), clamp((t - mid) / (1.0 - mid), 0.0, 1.0));
    }
    fill = over(veil, fill);

    // Band: a strip along the left or bottom edge, sheared with the shape.
    let band_w = shape.extra.z;
    var in_band = 0.0;
    if shape.mode.w > 0.5 && shape.mode.w < 1.5 {
        in_band = cover(q.x - (-half.x + band_w), aa);
    } else if shape.mode.w > 1.5 {
        in_band = cover((half.y - band_w) - p.y, aa);
    }
    var band_alpha = shape.band.a;
    if shape.extra.w > 0.0 {
        band_alpha = band_alpha * (1.0 - clamp(((q.x + half.x) / max(box_w, 1.0)) / shape.extra.w, 0.0, 1.0));
    }
    fill = over(premul(vec4<f32>(shape.band.rgb, band_alpha)) * in_band, fill);

    // Outside the shape: the ring (CSS `0 0 0 Npx`), then the drop shadow, both cut away under it.
    let ring = premul(shape.ring) * cover(d - shape.margin.w, aa) * step(0.01, shape.margin.w);
    let off = shape.extra.x;
    let ps = vec2<f32>(p.x + shear * (p.y - off), p.y - off);
    let blur = max(shape.extra.y, 0.5);
    let ds = sd_round_box(ps, half, r);
    let shadow = premul(shape.shadow) * (1.0 - smoothstep(-blur * 0.5, blur * 0.5, ds));
    let outside = over(ring, shadow) * (1.0 - inside);

    var out = fill * inside + outside;

    // Opacity and the list fade.
    var k = shape.geom.w;
    let f = shape.fade;
    if f.w > f.x {
        let y = in.position.y;
        k = k * smoothstep(f.x, f.y, y) * (1.0 - smoothstep(f.z, f.w, y));
    }
    out = out * k;
    if out.a <= 0.0001 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(to_linear(out.rgb / out.a), out.a);
}
