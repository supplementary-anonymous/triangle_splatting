struct VertexInput {
   @builtin(vertex_index) index: u32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};

@group(0)
@binding(0)
var tex_sampler: sampler;

@group(0)
@binding(1)
var tex: texture_2d<f32>;

@group(0)
@binding(2)
var prev_frame: texture_2d<f32>;

@group(1)
@binding(0)
var<uniform> globals: vec4<u32>;

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
    var result: VertexOutput;
    if (vertex.index == 0u) {
        result.position = vec4<f32>(-1.0, -1.0, 0.0, 1.0);
    } else if (vertex.index == 1u) {
        result.position = vec4<f32>(1.0, -1.0, 0.0, 1.0);
    } else if (vertex.index == 2u) {
        result.position = vec4<f32>(-1.0, 1.0, 0.0, 1.0);
    } else if (vertex.index == 3u) {
        result.position = vec4<f32>(1.0, 1.0, 0.0, 1.0);
    }
    return result;
}

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    let prev_rgbc = textureLoad(prev_frame, vec2<i32>(i32(vertex.position.x), i32(vertex.position.y)), 0);
    let dimensions = textureDimensions(tex);
    var rgb = vec3<f32>(0.0, 0.0, 0.0);
    let supersample = globals.x;
    for (var i: u32 = 0; i < supersample; i++) {
        for (var j: u32 = 0; j < supersample; j++) {
            let u = (f32(supersample) * vertex.position.x + f32(i)) / f32(dimensions.x);
            let v = (f32(supersample) * vertex.position.y + f32(j)) / f32(dimensions.y);
            let uv = vec2<f32>(u, v);
            rgb += textureSample(tex, tex_sampler, uv).xyz;
        }
    }
    rgb /= f32(supersample * supersample);
    let stale_camera = globals.y;
    if (stale_camera == 1u) {
        return vec4<f32>(rgb, 1.0);
    } else {
        let prev_rgb = prev_rgbc.xyz;
        let prev_count = prev_rgbc.w;
        if (prev_count > 1000000.0) {
            return prev_rgbc;
        }
        let new_rgb = (prev_rgb * prev_count + rgb) / (prev_count + 1.0);
        let new_count = prev_count + 1.0;
        return vec4<f32>(new_rgb, new_count);
    }
}