// USD Bridge - C++ Implementation
//
// Wraps Pixar's USD C++ API with a C interface for Rust FFI.

#include "usd_bridge.h"

#include <pxr/usd/usd/stage.h>
#include <pxr/usd/usd/primRange.h>
#include <pxr/usd/usdGeom/mesh.h>
#include <pxr/usd/usdGeom/pointInstancer.h>
#include <pxr/usd/usdGeom/xformCache.h>
#include <pxr/base/gf/matrix4f.h>
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

/// Internal stage representation
struct UsdBridgeStage {
    UsdStageRefPtr stage;
    std::vector<CachedMesh> meshes;
    std::vector<CachedInstancer> instancers;
    std::vector<CachedPrimInfo> all_prims;  // All prims in traversal order
    std::vector<std::string> root_paths;    // Direct children of pseudo-root
    std::vector<const char*> root_path_ptrs;
    bool cached;
    bool prims_cached;

    UsdBridgeStage() : cached(false), prims_cached(false) {}
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
            cached.vertices.reserve(points.size() * 3);
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

                cached.transforms.reserve(instance_transforms.size() * 16);
                for (const auto& mat : instance_transforms) {
                    float mat_data[16];
                    matrix_to_float16(mat, mat_data);
                    for (int i = 0; i < 16; ++i) {
                        cached.transforms.push_back(mat_data[i]);
                    }
                }
            }

            bridge->instancers.push_back(std::move(cached));
        }
    }

    bridge->cached = true;
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
    delete stage;  // Safe to delete nullptr
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
    out_data->path = mesh.path.c_str();
    out_data->vertices = mesh.vertices.data();
    out_data->vertex_count = mesh.vertices.size() / 3;
    out_data->indices = mesh.indices.data();
    out_data->index_count = mesh.indices.size();
    out_data->normals = mesh.normals.empty() ? nullptr : mesh.normals.data();
    out_data->normal_count = mesh.normals.size() / 3;

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
