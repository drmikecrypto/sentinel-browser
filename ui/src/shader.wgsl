// Vertex shader

struct Uniforms {
    screen_width: f32,
    screen_height: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct InstanceInput {
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) in_vertex_index: u32,
    instance: InstanceInput,
) -> VertexOutput {
    var out: VertexOutput;
    
    // Standard quad vertices (0,0) to (1,1)
    var vertices = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0)
    );
    
    let vertex = vertices[in_vertex_index];
    
    // Scale and translate
    let pixel_pos = instance.pos + vertex * instance.size;
    
    // Convert to NDC (-1 to 1)
    // x: 0..width -> -1..1
    // y: 0..height -> 1..-1 (flip Y for top-left origin)
    
    let ndc_x = (pixel_pos.x / uniforms.screen_width) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel_pos.y / uniforms.screen_height) * 2.0; 
    
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = instance.color;
    
    return out;
}

// Fragment shader

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
