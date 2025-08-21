@group(0)
@binding(0)
var point_sampler: sampler;

@group(0)
@binding(1)
var alpha_sigma_texture: texture_2d<f32>;

@group(0)
@binding(2)
var sh_texture: texture_2d<f32>;

struct Globals {
    fb_size: vec2<i32>,
    origin: vec3<f32>,
    num_tris: u32,
    seed: u32,
    vp: mat4x4<f32>,
    supersample: u32,
}

@group(1)
@binding(0)
var<uniform> globals: Globals;

struct VertexInput {
   @location(0) position: vec3<f32>,
   @builtin(vertex_index) index: u32,
};

struct VertexOutput {
    @location(0) uvws: vec4<f32>,
    @location(1) rgba: vec4<f32>,
    @location(2) @interpolate(flat) seed: u32,
    @builtin(position) position: vec4<f32>,
};

fn idx2vec2(idx: u32) -> vec2<i32> {
    return vec2<i32>(i32(idx % 8192u), i32(idx / 8192u));
}

fn sh2rgb(v: vec3<f32>, ti: u32) -> vec3<f32> {
    var b: array<f32, 16> = array<f32, 16>(
        0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0,
    );

    let vx2 = v.x * v.x;
    let vy2 = v.y * v.y;
    let vz2 = v.z * v.z;

    // zeroth order
    // (/ 1.0 (* 2.0 (sqrt pi)))
    b[0] = 0.28209479177387814;

    // first order
    // (/ (sqrt 3.0) (* 2 (sqrt pi)))
    let k1 = 0.4886025119029199;
    b[1] = -k1 * v.y;
    b[2] = k1 * v.z;
    b[3] = -k1 * v.x;

    // second order
    // (/ (sqrt 15.0) (* 2 (sqrt pi)))
    let k2 = 1.0925484305920792;
    // (/ (sqrt 5.0) (* 4 (sqrt  pi)))
    let k3 = 0.31539156525252005;
    // (/ (sqrt 15.0) (* 4 (sqrt pi)))
    let k4 = 0.5462742152960396;
    b[4] = k2 * v.y * v.x;
    b[5] = -k2 * v.y * v.z;
    b[6] = k3 * (3.0 * vz2 - 1.0);
    b[7] = -k2 * v.x * v.z;
    b[8] = k4 * (vx2 - vy2);

    // third order
    // (/ (* (sqrt 2) (sqrt 35)) (* 8 (sqrt pi)))
    let k5 = 0.5900435899266435;
    // (/ (sqrt 105) (* 2 (sqrt pi)))
    let k6 = 2.8906114426405543;
    // (/ (* (sqrt 2) (sqrt 21)) (* 8 (sqrt pi)))
    let k7 = 0.4570457994644658;
    // (/ (sqrt 7) (* 4 (sqrt pi)))
    let k8 = 0.37317633259011546;
    // (/ (sqrt 105) (* 4 (sqrt pi)))
    let k9 = 1.4453057213202771;
    b[9] = -k5 * v.y * (3.0 * vx2 - vy2);
    b[10] = k6 * v.y * v.x * v.z;
    b[11] = -k7 * v.y * (5.0 * vz2 - 1.0);
    b[12] = k8 * v.z * (5.0 * vz2 - 3.0);
    b[13] = -k7 * v.x * (5.0 * vz2 - 1.0);
    b[14] = k9 * v.z * (vx2 - vy2);
    b[15] = -k5 * v.x * (vx2 - 3.0 * vy2);

    let sh0 = textureLoad(sh_texture, idx2vec2(ti), 0);
    let sh1 = textureLoad(sh_texture, idx2vec2(ti + globals.num_tris), 0);
    let sh2 = textureLoad(sh_texture, idx2vec2(ti + 2 * globals.num_tris), 0);
    let sh3 = textureLoad(sh_texture, idx2vec2(ti + 3 * globals.num_tris), 0);
    let sh4 = textureLoad(sh_texture, idx2vec2(ti + 4 * globals.num_tris), 0);
    let sh5 = textureLoad(sh_texture, idx2vec2(ti + 5 * globals.num_tris), 0);
    let sh6 = textureLoad(sh_texture, idx2vec2(ti + 6 * globals.num_tris), 0);
    let sh7 = textureLoad(sh_texture, idx2vec2(ti + 7 * globals.num_tris), 0);
    let sh8 = textureLoad(sh_texture, idx2vec2(ti + 8 * globals.num_tris), 0);
    let sh9 = textureLoad(sh_texture, idx2vec2(ti + 9 * globals.num_tris), 0);
    let sh10 = textureLoad(sh_texture, idx2vec2(ti + 10 * globals.num_tris), 0);
    let sh11 = textureLoad(sh_texture, idx2vec2(ti + 11 * globals.num_tris), 0);

    var rgb = sh0.xyz * b[0];

    let c1 = vec3<f32>(sh0.w, sh1.x, sh1.y);
    rgb += c1 * b[1];
    let c2 = vec3<f32>(sh1.z, sh1.w, sh2.x);
    rgb += c2 * b[2];
    let c3 = vec3<f32>(sh2.y, sh2.z, sh2.w);
    rgb += c3 * b[3];
    let c4 = vec3<f32>(sh3.x, sh3.y, sh3.z);
    rgb += c4 * b[4];
    let c5 = vec3<f32>(sh3.w, sh4.x, sh4.y);
    rgb += c5 * b[5];
    let c6 = vec3<f32>(sh4.z, sh4.w, sh5.x);
    rgb += c6 * b[6];
    let c7 = vec3<f32>(sh5.y, sh5.z, sh5.w);
    rgb += c7 * b[7];
    let c8 = vec3<f32>(sh6.x, sh6.y, sh6.z);
    rgb += c8 * b[8];
    let c9 = vec3<f32>(sh6.w, sh7.x, sh7.y);
    rgb += c9 * b[9];
    let c10 = vec3<f32>(sh7.z, sh7.w, sh8.x);
    rgb += c10 * b[10];
    let c11 = vec3<f32>(sh8.y, sh8.z, sh8.w);
    rgb += c11 * b[11];
    let c12 = vec3<f32>(sh9.x, sh9.y, sh9.z);
    rgb += c12 * b[12];
    let c13 = vec3<f32>(sh9.w, sh10.x, sh10.y);
    rgb += c13 * b[13];
    let c14 = vec3<f32>(sh10.z, sh10.w, sh11.x);
    rgb += c14 * b[14];
    let c15 = vec3<f32>(sh11.y, sh11.z, sh11.w);
    rgb += c15 * b[15];

    return rgb + vec3<f32>(0.5, 0.5, 0.5);
}

fn hash(seed: u32) -> u32 {
    var x = seed;
    x ^= x >> 17u;
    x *= 0xed5ad4bbu;
    x ^= x >> 11u;
    x *= 0xac4c1b51u;
    x ^= x >> 15u;
    x *= 0x31848babu;
    x ^= x >> 14u;
    return x;
}

fn rand(seed: u32) -> f32 {
    return f32(hash(seed)) / 4294967295.0;
}

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
    let triangle_index = vertex.index / 3u;
    let tex_coord = idx2vec2(triangle_index);
    let alpha_sigma = textureLoad(alpha_sigma_texture, tex_coord, 0).xy;
    let alpha = alpha_sigma.x;
    let sigma = alpha_sigma.y;

    var v = vertex.position - globals.origin;
    v /= length(v);

    let rgb = sh2rgb(v, triangle_index);

    var result: VertexOutput;
    result.rgba = vec4<f32>(rgb, alpha);
    if (vertex.index % 3u == 0u) {
        result.uvws = vec4<f32>(3.0, 0.0, 0.0, sigma);
    } else if (vertex.index % 3u == 1u) {
        result.uvws = vec4<f32>(0.0, 3.0, 0.0, sigma);
    } else {
        result.uvws = vec4<f32>(0.0, 0.0, 3.0, sigma);
    }
    result.position = globals.vp * vec4<f32>(vertex.position, 1.0);
    result.seed = hash(globals.seed) ^ hash(triangle_index);
    return result;
}

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    var seed = u32(vertex.position.x + vertex.position.y * f32(globals.fb_size.x));
    seed ^= vertex.seed;
    var u = rand(seed);

    let phi = min(vertex.uvws.x, min(vertex.uvws.y, vertex.uvws.z));
    let a = pow(phi, vertex.uvws.w) * vertex.rgba.w;

    if (a < u) {
        discard;
    }

    let rgba = vec4<f32>(vertex.rgba.xyz, 1.0);

    return rgba;
}