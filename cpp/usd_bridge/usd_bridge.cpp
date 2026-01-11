// USD Bridge - C++ Implementation
//
// Wraps Pixar's USD C++ API with a C interface for Rust FFI.

#include "usd_bridge.h"

#include <pxr/usd/usd/stage.h>
#include <pxr/usd/usd/primRange.h>
#include <pxr/usd/usdGeom/mesh.h>
#include <pxr/usd/usdGeom/pointInstancer.h>
#include <pxr/usd/usdGeom/xformCache.h>
#include <pxr/usd/usdGeom/primvarsAPI.h>
#include <pxr/usd/usdShade/material.h>
#include <pxr/usd/usdShade/materialBindingAPI.h>
#include <pxr/usd/usdShade/shader.h>
#include <pxr/base/gf/matrix4f.h>
#include <pxr/base/gf/vec2f.h>
#include <pxr/base/gf/vec3f.h>
#include <pxr/base/gf/quath.h>
#include <pxr/base/vt/array.h>

#include <vector>
#include <string>
#include <memory>

PXR_NAMESPACE_USING_DIRECTIVE

// ============================================================================
// Internal Data Structures
// ============================================================================

/// Cached mesh data for FFI transfer
struct CachedMesh {
    std::string path;
    std::vector<float> vertices;
    std::vector<uint32_t> indices;
    std::vector<float> normals;
    std::vector<float> uvs;  // u,v pairs from primvars:st
    GfMatrix4d transform;
};

/// Cached instancer data for FFI transfer
struct CachedInstancer {
    std::string path;
    std::vector<std::string> prototype_paths;
    std::vector<const char*> prototype_path_ptrs;  // For C API
    std::vector<float> transforms;
    std::vector<int32_t> proto_indices;
};

/// Cached prim info for scene browser
struct CachedPrimInfo {
    std::string path;
    std::string type_name;
    bool is_active;
    bool has_children;
    size_t child_count;
    std::vector<std::string> child_paths;
    std::vector<const char*> child_path_ptrs;  // For C API
};

/// Cached material data for FFI transfer (UsdPreviewSurface)
struct CachedMaterial {
    std::string path;
    float diffuse_color[3];
    float metallic;
    float roughness;
    float specular;
    float opacity;
    float emissive_color[3];
    std::string diffuse_texture;
    std::string roughness_texture;
    std::string metallic_texture;
    std::string normal_texture;
    std::string emissive_texture;
    std::string material_path_for_mesh;  // Per-mesh material binding
};

/// Internal stage representation
struct UsdBridgeStage {
    UsdStageRefPtr stage;
    std::vector<CachedMesh> meshes;
    std::vector<CachedInstancer> instancers;
    std::vector<CachedMaterial> materials;
    std::vector<std::string> mesh_material_paths;  // Material path per mesh
    std::vector<CachedPrimInfo> all_prims;  // All prims in traversal order
    std::vector<std::string> root_paths;    // Direct children of pseudo-root
    std::vector<const char*> root_path_ptrs;
    bool cached;
    bool prims_cached;
    bool materials_cached;

    UsdBridgeStage() : cached(false), prims_cached(false), materials_cached(false) {}

    ~UsdBridgeStage() {
        // Clear cached data to ensure proper cleanup
        meshes.clear();
        instancers.clear();
        materials.clear();
        mesh_material_paths.clear();
        all_prims.clear();
        root_paths.clear();
        root_path_ptrs.clear();
        // stage RefPtr will auto-release
    }
};

// ============================================================================
// Helper Functions
// ============================================================================

/// Triangulate a polygon mesh (fan triangulation for n-gons)
static void triangulate_mesh(
    const VtArray<int>& face_vertex_counts,
    const VtArray<int>& face_vertex_indices,
    std::vector<uint32_t>& out_indices
) {
    out_indices.clear();
    size_t idx_offset = 0;

    for (int face_size : face_vertex_counts) {
        if (face_size < 3) {
            idx_offset += face_size;
            continue;
        }

        // Fan triangulation: (0,1,2), (0,2,3), (0,3,4), ...
        for (int i = 1; i < face_size - 1; ++i) {
            out_indices.push_back(static_cast<uint32_t>(face_vertex_indices[idx_offset]));
            out_indices.push_back(static_cast<uint32_t>(face_vertex_indices[idx_offset + i]));
            out_indices.push_back(static_cast<uint32_t>(face_vertex_indices[idx_offset + i + 1]));
        }
        idx_offset += face_size;
    }
}

/// Convert GfMatrix4d to column-major float array
static void matrix_to_float16(const GfMatrix4d& mat, float* out) {
    GfMatrix4f matf(mat);
    const double* data = mat.GetArray();
    for (int i = 0; i < 16; ++i) {
        out[i] = static_cast<float>(data[i]);
    }
}

/// Cache all prim info for scene browser
static void cache_prim_data(UsdBridgeStage* bridge) {
    if (bridge->prims_cached) return;

    bridge->all_prims.clear();
    bridge->root_paths.clear();
    bridge->root_path_ptrs.clear();

    // Get root prims (direct children of pseudo-root)
    UsdPrim pseudo_root = bridge->stage->GetPseudoRoot();
    for (const UsdPrim& child : pseudo_root.GetChildren()) {
        bridge->root_paths.push_back(child.GetPath().GetString());
    }
    for (const auto& path : bridge->root_paths) {
        bridge->root_path_ptrs.push_back(path.c_str());
    }

    // Traverse all prims in depth-first order
    for (const UsdPrim& prim : bridge->stage->Traverse()) {
        CachedPrimInfo info;
        info.path = prim.GetPath().GetString();
        info.type_name = prim.GetTypeName().GetString();
        info.is_active = prim.IsActive();
        
        // Get children
        for (const UsdPrim& child : prim.GetChildren()) {
            info.child_paths.push_back(child.GetPath().GetString());
        }
        for (const auto& child_path : info.child_paths) {
            info.child_path_ptrs.push_back(child_path.c_str());
        }
        info.has_children = !info.child_paths.empty();
        info.child_count = info.child_paths.size();

        bridge->all_prims.push_back(std::move(info));
    }

    bridge->prims_cached = true;
}

/// Cache all mesh and instancer data from the stage
static void cache_stage_data(UsdBridgeStage* bridge) {
    if (bridge->cached) return;

    UsdGeomXformCache xform_cache;

    // Traverse all prims
    for (const UsdPrim& prim : bridge->stage->Traverse()) {
        // Check for UsdGeomMesh
        if (prim.IsA<UsdGeomMesh>()) {
            UsdGeomMesh mesh(prim);
            CachedMesh cached;
            cached.path = prim.GetPath().GetString();

            // Get points at earliest authored time for animated geometry
            VtArray<GfVec3f> points;
            UsdTimeCode timeCode = UsdTimeCode::EarliestTime();
            mesh.GetPointsAttr().Get(&points, timeCode);
            
            // Pre-allocate to exact size to minimize memory overhead
            cached.vertices.reserve(points.size() * 3);
            cached.vertices.shrink_to_fit();
            for (const auto& p : points) {
                cached.vertices.push_back(p[0]);
                cached.vertices.push_back(p[1]);
                cached.vertices.push_back(p[2]);
            }

            // Get face topology and triangulate
            VtArray<int> face_vertex_counts;
            VtArray<int> face_vertex_indices;
            mesh.GetFaceVertexCountsAttr().Get(&face_vertex_counts, timeCode);
            mesh.GetFaceVertexIndicesAttr().Get(&face_vertex_indices, timeCode);
            triangulate_mesh(face_vertex_counts, face_vertex_indices, cached.indices);

            // Get normals (optional)
            VtArray<GfVec3f> normals;
            if (mesh.GetNormalsAttr().Get(&normals, timeCode)) {
                cached.normals.reserve(normals.size() * 3);
                for (const auto& n : normals) {
                    cached.normals.push_back(n[0]);
                    cached.normals.push_back(n[1]);
                    cached.normals.push_back(n[2]);
                }
            }

            // Get UV coordinates from primvars:st (optional)
            UsdGeomPrimvarsAPI primvarsAPI(mesh);
            UsdGeomPrimvar stPrimvar = primvarsAPI.GetPrimvar(TfToken("st"));
            if (stPrimvar) {
                VtArray<GfVec2f> uvs;
                if (stPrimvar.Get(&uvs, timeCode)) {
                    cached.uvs.reserve(uvs.size() * 2);
                    for (const auto& uv : uvs) {
                        cached.uvs.push_back(uv[0]);
                        cached.uvs.push_back(uv[1]);
                    }
                }
            }

            // Get world transform
            cached.transform = xform_cache.GetLocalToWorldTransform(prim);

            bridge->meshes.push_back(std::move(cached));
        }

        // Check for UsdGeomPointInstancer
        if (prim.IsA<UsdGeomPointInstancer>()) {
            UsdGeomPointInstancer instancer(prim);
            CachedInstancer cached;
            cached.path = prim.GetPath().GetString();

            // Get prototype relationships (USD 25.x API: output parameter)
            SdfPathVector proto_paths;
            instancer.GetPrototypesRel().GetForwardedTargets(&proto_paths);
            for (const auto& proto_path : proto_paths) {
                cached.prototype_paths.push_back(proto_path.GetString());
            }
            // Build C-string pointers
            for (const auto& path_str : cached.prototype_paths) {
                cached.prototype_path_ptrs.push_back(path_str.c_str());
            }

            // Get proto indices
            VtArray<int> proto_indices;
            instancer.GetProtoIndicesAttr().Get(&proto_indices);
            cached.proto_indices.assign(proto_indices.begin(), proto_indices.end());

            // Compute instance transforms
            VtArray<GfMatrix4d> instance_transforms;
            if (instancer.ComputeInstanceTransformsAtTime(
                    &instance_transforms,
                    UsdTimeCode::Default(),
                    UsdTimeCode::Default())) {

                // Pre-allocate exact size for transforms
                cached.transforms.reserve(instance_transforms.size() * 16);
                for (const auto& mat : instance_transforms) {
                    float mat_data[16];
                    matrix_to_float16(mat, mat_data);
                    for (int i = 0; i < 16; ++i) {
                        cached.transforms.push_back(mat_data[i]);
                    }
                }
                cached.transforms.shrink_to_fit();
            }

            bridge->instancers.push_back(std::move(cached));
        }
    }

    bridge->cached = true;
}

/// Helper to extract a texture path from a shader input connection
static std::string get_texture_path(const UsdShadeInput& input) {
    if (!input) return "";

    // Check for a connection to a texture reader
    SdfPathVector connections;
    input.GetRawConnectedSourcePaths(&connections);

    for (const auto& conn_path : connections) {
        // The connection target is usually something like /Material/Shader.outputs:rgb
        // We need to find the shader prim and get its file attribute
        SdfPath prim_path = conn_path.GetPrimPath();
        UsdPrim shader_prim = input.GetPrim().GetStage()->GetPrimAtPath(prim_path);
        if (!shader_prim) continue;

        UsdShadeShader shader(shader_prim);
        if (!shader) continue;

        // Check if this is a UsdUVTexture
        TfToken shader_id;
        shader.GetIdAttr().Get(&shader_id);
        if (shader_id == TfToken("UsdUVTexture")) {
            // Get the file input
            UsdShadeInput file_input = shader.GetInput(TfToken("file"));
            if (file_input) {
                SdfAssetPath asset_path;
                if (file_input.Get(&asset_path)) {
                    return asset_path.GetResolvedPath().empty()
                        ? asset_path.GetAssetPath()
                        : asset_path.GetResolvedPath();
                }
            }
        }
    }
    return "";
}

/// Cache all material data from the stage
static void cache_material_data(UsdBridgeStage* bridge) {
    if (bridge->materials_cached) return;

    bridge->materials.clear();

    // Find all UsdShadeMaterial prims
    for (const UsdPrim& prim : bridge->stage->Traverse()) {
        if (!prim.IsA<UsdShadeMaterial>()) continue;

        UsdShadeMaterial material(prim);
        CachedMaterial cached;
        cached.path = prim.GetPath().GetString();

        // Initialize defaults
        cached.diffuse_color[0] = 0.5f;
        cached.diffuse_color[1] = 0.5f;
        cached.diffuse_color[2] = 0.5f;
        cached.metallic = 0.0f;
        cached.roughness = 0.5f;
        cached.specular = 0.5f;
        cached.opacity = 1.0f;
        cached.emissive_color[0] = 0.0f;
        cached.emissive_color[1] = 0.0f;
        cached.emissive_color[2] = 0.0f;

        // Get the surface shader output
        UsdShadeOutput surface_output = material.GetSurfaceOutput();
        if (!surface_output) {
            bridge->materials.push_back(std::move(cached));
            continue;
        }

        // Find connected shader
        SdfPathVector connections;
        surface_output.GetRawConnectedSourcePaths(&connections);
        if (connections.empty()) {
            bridge->materials.push_back(std::move(cached));
            continue;
        }

        SdfPath shader_path = connections[0].GetPrimPath();
        UsdPrim shader_prim = bridge->stage->GetPrimAtPath(shader_path);
        if (!shader_prim) {
            bridge->materials.push_back(std::move(cached));
            continue;
        }

        UsdShadeShader shader(shader_prim);
        if (!shader) {
            bridge->materials.push_back(std::move(cached));
            continue;
        }

        // Check if it's UsdPreviewSurface
        TfToken shader_id;
        shader.GetIdAttr().Get(&shader_id);
        if (shader_id != TfToken("UsdPreviewSurface")) {
            bridge->materials.push_back(std::move(cached));
            continue;
        }

        // Extract UsdPreviewSurface parameters
        UsdShadeInput input;

        // Diffuse color
        input = shader.GetInput(TfToken("diffuseColor"));
        if (input) {
            GfVec3f color;
            if (input.Get(&color)) {
                cached.diffuse_color[0] = color[0];
                cached.diffuse_color[1] = color[1];
                cached.diffuse_color[2] = color[2];
            }
            cached.diffuse_texture = get_texture_path(input);
        }

        // Metallic
        input = shader.GetInput(TfToken("metallic"));
        if (input) {
            input.Get(&cached.metallic);
            cached.metallic_texture = get_texture_path(input);
        }

        // Roughness
        input = shader.GetInput(TfToken("roughness"));
        if (input) {
            input.Get(&cached.roughness);
            cached.roughness_texture = get_texture_path(input);
        }

        // Specular (ior in UsdPreviewSurface, but we use specular for simplicity)
        input = shader.GetInput(TfToken("specularColor"));
        if (input) {
            GfVec3f spec;
            if (input.Get(&spec)) {
                cached.specular = (spec[0] + spec[1] + spec[2]) / 3.0f;
            }
        }

        // Opacity
        input = shader.GetInput(TfToken("opacity"));
        if (input) {
            input.Get(&cached.opacity);
        }

        // Emissive color
        input = shader.GetInput(TfToken("emissiveColor"));
        if (input) {
            GfVec3f emissive;
            if (input.Get(&emissive)) {
                cached.emissive_color[0] = emissive[0];
                cached.emissive_color[1] = emissive[1];
                cached.emissive_color[2] = emissive[2];
            }
            cached.emissive_texture = get_texture_path(input);
        }

        // Normal map
        input = shader.GetInput(TfToken("normal"));
        if (input) {
            cached.normal_texture = get_texture_path(input);
        }

        bridge->materials.push_back(std::move(cached));
    }

    // Also collect mesh-to-material bindings
    bridge->mesh_material_paths.clear();
    for (const auto& mesh : bridge->meshes) {
        UsdPrim mesh_prim = bridge->stage->GetPrimAtPath(SdfPath(mesh.path));
        std::string mat_path;

        if (mesh_prim) {
            UsdShadeMaterialBindingAPI binding_api(mesh_prim);
            if (binding_api) {
                UsdShadeMaterial bound_material = binding_api.ComputeBoundMaterial();
                if (bound_material) {
                    mat_path = bound_material.GetPath().GetString();
                }
            }
        }

        bridge->mesh_material_paths.push_back(mat_path);
    }

    bridge->materials_cached = true;
}

// ============================================================================
// C API Implementation
// ============================================================================

const char* usd_bridge_error_message(UsdBridgeError error) {
    switch (error) {
        case USD_BRIDGE_SUCCESS: return "Success";
        case USD_BRIDGE_ERROR_NULL_POINTER: return "Null pointer argument";
        case USD_BRIDGE_ERROR_FILE_NOT_FOUND: return "File not found";
        case USD_BRIDGE_ERROR_INVALID_STAGE: return "Invalid stage handle";
        case USD_BRIDGE_ERROR_INVALID_PRIM: return "Invalid prim or index";
        case USD_BRIDGE_ERROR_OUT_OF_MEMORY: return "Out of memory";
        default: return "Unknown error";
    }
}

UsdBridgeError usd_bridge_open_stage(const char* path, UsdBridgeStage** out_stage) {
    if (!path || !out_stage) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    try {
        UsdStageRefPtr stage = UsdStage::Open(path);
        if (!stage) {
            return USD_BRIDGE_ERROR_FILE_NOT_FOUND;
        }

        auto* bridge = new UsdBridgeStage();
        bridge->stage = stage;
        *out_stage = bridge;
        return USD_BRIDGE_SUCCESS;

    } catch (...) {
        return USD_BRIDGE_ERROR_UNKNOWN;
    }
}

void usd_bridge_close_stage(UsdBridgeStage* stage) {
    if (stage) {
        // Log memory usage before cleanup
        size_t mesh_mem = 0;
        for (const auto& mesh : stage->meshes) {
            mesh_mem += mesh.vertices.capacity() * sizeof(float);
            mesh_mem += mesh.indices.capacity() * sizeof(uint32_t);
            mesh_mem += mesh.normals.capacity() * sizeof(float);
        }
        if (mesh_mem > 0) {
            // Optional: log memory being freed
            // std::cerr << "Freeing ~" << (mesh_mem / 1024 / 1024) << "MB of cached USD data\n";
        }
    }
    delete stage;  // Safe to delete nullptr
}

void usd_bridge_clear_cache(UsdBridgeStage* stage) {
    if (!stage) return;
    
    // Clear mesh and instancer caches to free memory
    stage->meshes.clear();
    stage->meshes.shrink_to_fit();
    stage->instancers.clear();
    stage->instancers.shrink_to_fit();
    stage->all_prims.clear();
    stage->all_prims.shrink_to_fit();
    stage->root_paths.clear();
    stage->root_path_ptrs.clear();
    
    // Reset cache flags
    stage->cached = false;
    stage->prims_cached = false;
}

UsdBridgeError usd_bridge_get_mesh_count(
    const UsdBridgeStage* stage,
    size_t* out_count
) {
    if (!stage || !out_count) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    // Need to cache first (const_cast for lazy caching)
    cache_stage_data(const_cast<UsdBridgeStage*>(stage));
    *out_count = stage->meshes.size();
    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_instancer_count(
    const UsdBridgeStage* stage,
    size_t* out_count
) {
    if (!stage || !out_count) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    cache_stage_data(const_cast<UsdBridgeStage*>(stage));
    *out_count = stage->instancers.size();
    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_mesh(
    const UsdBridgeStage* stage,
    size_t index,
    UsdBridgeMeshData* out_data
) {
    if (!stage || !out_data) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    cache_stage_data(const_cast<UsdBridgeStage*>(stage));

    if (index >= stage->meshes.size()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    const CachedMesh& mesh = stage->meshes[index];
    // IMPORTANT: These pointers are only valid while stage exists!
    // Rust side must copy data immediately
    out_data->path = mesh.path.c_str();
    out_data->vertices = mesh.vertices.data();
    out_data->vertex_count = mesh.vertices.size() / 3;
    out_data->indices = mesh.indices.data();
    out_data->index_count = mesh.indices.size();
    out_data->normals = mesh.normals.empty() ? nullptr : mesh.normals.data();
    out_data->normal_count = mesh.normals.size() / 3;
    out_data->uvs = mesh.uvs.empty() ? nullptr : mesh.uvs.data();
    out_data->uv_count = mesh.uvs.size() / 2;

    // Copy transform
    float mat_data[16];
    matrix_to_float16(mesh.transform, mat_data);
    for (int i = 0; i < 16; ++i) {
        out_data->transform[i] = mat_data[i];
    }

    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_instancer(
    const UsdBridgeStage* stage,
    size_t index,
    UsdBridgeInstancerData* out_data
) {
    if (!stage || !out_data) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    cache_stage_data(const_cast<UsdBridgeStage*>(stage));

    if (index >= stage->instancers.size()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    const CachedInstancer& instancer = stage->instancers[index];
    out_data->path = instancer.path.c_str();
    out_data->prototype_paths = instancer.prototype_path_ptrs.data();
    out_data->prototype_count = instancer.prototype_paths.size();
    out_data->transforms = instancer.transforms.data();
    out_data->instance_count = instancer.transforms.size() / 16;
    out_data->proto_indices = instancer.proto_indices.data();

    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_material_count(
    const UsdBridgeStage* stage,
    size_t* out_count
) {
    if (!stage || !out_count) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    // Ensure mesh data is cached first (needed for material bindings)
    cache_stage_data(const_cast<UsdBridgeStage*>(stage));
    cache_material_data(const_cast<UsdBridgeStage*>(stage));

    *out_count = stage->materials.size();
    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_material(
    const UsdBridgeStage* stage,
    size_t index,
    UsdBridgeMaterialData* out_data
) {
    if (!stage || !out_data) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    cache_stage_data(const_cast<UsdBridgeStage*>(stage));
    cache_material_data(const_cast<UsdBridgeStage*>(stage));

    if (index >= stage->materials.size()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    const CachedMaterial& mat = stage->materials[index];
    out_data->path = mat.path.c_str();
    out_data->diffuse_color[0] = mat.diffuse_color[0];
    out_data->diffuse_color[1] = mat.diffuse_color[1];
    out_data->diffuse_color[2] = mat.diffuse_color[2];
    out_data->metallic = mat.metallic;
    out_data->roughness = mat.roughness;
    out_data->specular = mat.specular;
    out_data->opacity = mat.opacity;
    out_data->emissive_color[0] = mat.emissive_color[0];
    out_data->emissive_color[1] = mat.emissive_color[1];
    out_data->emissive_color[2] = mat.emissive_color[2];
    out_data->diffuse_texture = mat.diffuse_texture.empty() ? nullptr : mat.diffuse_texture.c_str();
    out_data->roughness_texture = mat.roughness_texture.empty() ? nullptr : mat.roughness_texture.c_str();
    out_data->metallic_texture = mat.metallic_texture.empty() ? nullptr : mat.metallic_texture.c_str();
    out_data->normal_texture = mat.normal_texture.empty() ? nullptr : mat.normal_texture.c_str();
    out_data->emissive_texture = mat.emissive_texture.empty() ? nullptr : mat.emissive_texture.c_str();

    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_mesh_material_path(
    const UsdBridgeStage* stage,
    size_t mesh_index,
    const char** out_path
) {
    if (!stage || !out_path) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    cache_stage_data(const_cast<UsdBridgeStage*>(stage));
    cache_material_data(const_cast<UsdBridgeStage*>(stage));

    if (mesh_index >= stage->mesh_material_paths.size()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    *out_path = stage->mesh_material_paths[mesh_index].c_str();
    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_export_stage(
    const UsdBridgeStage* stage,
    const char* path
) {
    if (!stage || !path) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    try {
        if (!stage->stage->Export(path)) {
            return USD_BRIDGE_ERROR_UNKNOWN;
        }
        return USD_BRIDGE_SUCCESS;
    } catch (...) {
        return USD_BRIDGE_ERROR_UNKNOWN;
    }
}

// ============================================================================
// Prim Traversal API Implementation
// ============================================================================

UsdBridgeError usd_bridge_get_prim_count(
    const UsdBridgeStage* stage,
    size_t* out_count
) {
    if (!stage || !out_count) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    cache_prim_data(const_cast<UsdBridgeStage*>(stage));
    *out_count = stage->all_prims.size();
    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_prim_info(
    const UsdBridgeStage* stage,
    size_t index,
    UsdBridgePrimInfo* out_info
) {
    if (!stage || !out_info) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    cache_prim_data(const_cast<UsdBridgeStage*>(stage));

    if (index >= stage->all_prims.size()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    const CachedPrimInfo& info = stage->all_prims[index];
    out_info->path = info.path.c_str();
    out_info->type_name = info.type_name.c_str();
    out_info->is_active = info.is_active ? 1 : 0;
    out_info->has_children = info.has_children ? 1 : 0;
    out_info->child_count = info.child_count;

    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_root_prim_count(
    const UsdBridgeStage* stage,
    size_t* out_count
) {
    if (!stage || !out_count) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    cache_prim_data(const_cast<UsdBridgeStage*>(stage));
    *out_count = stage->root_paths.size();
    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_root_prim_path(
    const UsdBridgeStage* stage,
    size_t index,
    const char** out_path
) {
    if (!stage || !out_path) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    cache_prim_data(const_cast<UsdBridgeStage*>(stage));

    if (index >= stage->root_paths.size()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    *out_path = stage->root_path_ptrs[index];
    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_children_count(
    const UsdBridgeStage* stage,
    const char* parent_path,
    size_t* out_count
) {
    if (!stage || !parent_path || !out_count) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    cache_prim_data(const_cast<UsdBridgeStage*>(stage));

    // Handle pseudo-root case
    std::string path_str(parent_path);
    if (path_str == "/" || path_str.empty()) {
        *out_count = stage->root_paths.size();
        return USD_BRIDGE_SUCCESS;
    }

    // Find the prim in our cache
    for (const auto& info : stage->all_prims) {
        if (info.path == path_str) {
            *out_count = info.child_count;
            return USD_BRIDGE_SUCCESS;
        }
    }

    return USD_BRIDGE_ERROR_INVALID_PRIM;
}

UsdBridgeError usd_bridge_get_child_path(
    const UsdBridgeStage* stage,
    const char* parent_path,
    size_t index,
    const char** out_path
) {
    if (!stage || !parent_path || !out_path) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    cache_prim_data(const_cast<UsdBridgeStage*>(stage));

    std::string path_str(parent_path);
    
    // Handle pseudo-root case
    if (path_str == "/" || path_str.empty()) {
        if (index >= stage->root_paths.size()) {
            return USD_BRIDGE_ERROR_INVALID_PRIM;
        }
        *out_path = stage->root_path_ptrs[index];
        return USD_BRIDGE_SUCCESS;
    }

    // Find the prim in our cache
    for (const auto& info : stage->all_prims) {
        if (info.path == path_str) {
            if (index >= info.child_paths.size()) {
                return USD_BRIDGE_ERROR_INVALID_PRIM;
            }
            *out_path = info.child_path_ptrs[index];
            return USD_BRIDGE_SUCCESS;
        }
    }

    return USD_BRIDGE_ERROR_INVALID_PRIM;
}

UsdBridgeError usd_bridge_get_prim_info_by_path(
    const UsdBridgeStage* stage,
    const char* path,
    UsdBridgePrimInfo* out_info
) {
    if (!stage || !path || !out_info) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    cache_prim_data(const_cast<UsdBridgeStage*>(stage));

    std::string path_str(path);
    for (const auto& info : stage->all_prims) {
        if (info.path == path_str) {
            out_info->path = info.path.c_str();
            out_info->type_name = info.type_name.c_str();
            out_info->is_active = info.is_active ? 1 : 0;
            out_info->has_children = info.has_children ? 1 : 0;
            out_info->child_count = info.child_count;
            return USD_BRIDGE_SUCCESS;
        }
    }

    return USD_BRIDGE_ERROR_INVALID_PRIM;
}
