struct VertexInput {
   @builtin(vertex_index) index: u32,
};

struct VertexOutput {
    @location(0) uv: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
    var result: VertexOutput;
    if (vertex.index == 0u) {
        result.uv = vec2<f32>(0.0, 1.0);
        result.position = vec4<f32>(-1.0, -1.0, 0.0, 1.0);
    } else if (vertex.index == 1u) {
        result.uv = vec2<f32>(1.0, 1.0);
        result.position = vec4<f32>(1.0, -1.0, 0.0, 1.0);
    } else if (vertex.index == 2u) {
        result.uv = vec2<f32>(0.0, 0.0);
        result.position = vec4<f32>(-1.0, 1.0, 0.0, 1.0);
    } else if (vertex.index == 3u) {
        result.uv = vec2<f32>(1.0, 0.0);
        result.position = vec4<f32>(1.0, 1.0, 0.0, 1.0);
    }
    return result;
}

@group(0)
@binding(0)
var tex_sampler: sampler;

@group(0)
@binding(1)
var tex: texture_2d<f32>;

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    let rgb = textureSample(tex, tex_sampler, vertex.uv).xyz;
    return vec4<f32>(rgb, 1.0);
}