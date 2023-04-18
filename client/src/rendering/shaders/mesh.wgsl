#import bevy_pbr::mesh_view_bindings
#import bevy_pbr::mesh_bindings

// NOTE: Bindings must come before functions that use them!
#import bevy_pbr::mesh_functions

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // This is bit packed, first 2 bits are uv, last 19 are block texture index
    //@location(2) uv: u32,
    @location(2) uv: vec2<f32>,
#ifdef VERTEX_TANGENTS
    @location(3) tangent: vec4<f32>,
#endif
#ifdef VERTEX_COLORS
    @location(4) color: vec4<f32>,
#endif
#ifdef SKINNED
    @location(5) joint_indices: vec4<u32>,
    @location(6) joint_weights: vec4<f32>,
#endif
    @location(7) texture_index: i32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) texture_index: i32,
#ifdef VERTEX_TANGENTS
    @location(4) world_tangent: vec4<f32>,
#endif
};

// Note: 0,0 is top left corner
//let UVS: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
//    vec2<f32>(0.0, 1.0),
//    vec2<f32>(0.0, 0.0),
//    vec2<f32>(1.0, 1.0),
//    vec2<f32>(1.0, 0.0),
//);
const UVS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 1.0),
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0),
);

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    //let index: u32 = vertex.uv >> 29u;
    //if (index == 0u) {
    //    out.uv = UVS[0];
    //} else if (index == 1u) {
    //    out.uv = UVS[1];
    //} else if (index == 2u) {
    //    out.uv = UVS[2];
    //} else if (index == 3u) {
    //    out.uv = UVS[3];
    //} else if (index == 4u) {
    //    out.uv = UVS[4];
    //} else if (index == 5u) {
    //    out.uv = UVS[5];
    //}
    out.uv = vertex.uv;

    //out.texture_index = i32(vertex.uv & 0x0007FFFFu);
    out.texture_index = vertex.texture_index;

    out.world_position = mesh_position_local_to_world(mesh.model, vec4<f32>(vertex.position, 1.0));
    out.clip_position = mesh_position_world_to_clip(out.world_position);
    out.world_normal = mesh_normal_local_to_world(vertex.normal);
#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_tangent_local_to_world(mesh.model, vertex.tangent);
#endif
    return out;
}
