#version 450

layout(local_size_x = 8, local_size_y = 8) in;

layout(push_constant, std140) uniform PushConstants {
  layout(offset = 0) uint frame_index;
} constants;

layout(rgba32f, binding = 0) uniform image2D present;

layout(binding = 1) buffer Dft {
    float data[];
} dft;

void main() {

    ivec2 ipixel_coords = ivec2(gl_GlobalInvocationID.xy);
    ivec2 iimage_size = ivec2(gl_NumWorkGroups.xy * gl_WorkGroupSize.xy);

    vec3 add_color = fract(constants.frame_index * 0.1203948230 * vec3(float(ipixel_coords.x) / iimage_size.x, float(ipixel_coords.y) / iimage_size.y, 0));
    vec3 load_color = imageLoad(present, ipixel_coords).rgb;

    if (constants.frame_index % 60 < 60) {
        imageStore(present, ipixel_coords, vec4(add_color, 1));
    } else {
        imageStore(present, ipixel_coords, vec4(load_color, 1));
    }
}
