// USD Bridge - C++ Implementation
//
// Wraps Pixar's USD C++ API with a C interface for Rust FFI.

#include "usd_bridge.h"

#include <pxr/usd/usd/stage.h>
#include <pxr/usd/usd/primRange.h>
#include <pxr/usd/usdGeom/mesh.h>
#include <pxr/usd/usdGeom/pointInstancer.h>
#include <pxr/usd/usdGeom/xformCache.h>
#include <pxr/usd/usdGeom/camera.h>
#include <pxr/usd/usdGeom/xformable.h>
#include <pxr/usd/usdGeom/primvarsAPI.h>
#include <pxr/usd/usdShade/material.h>
#include <pxr/usd/usdShade/materialBindingAPI.h>
#include <pxr/usd/usdShade/shader.h>
#include <pxr/usd/usdLux/distantLight.h>
#include <pxr/usd/usdLux/sphereLight.h>
#include <pxr/usd/usdLux/rectLight.h>
#include <pxr/usd/usdLux/domeLight.h>
#include <pxr/usd/usdLux/cylinderLight.h>
#include <pxr/usd/usdLux/diskLight.h>
#include <pxr/usd/usdLux/shapingAPI.h>
#include <pxr/usd/usdGeom/points.h>
#include <pxr/base/gf/matrix4f.h>
#include <pxr/base/gf/vec2f.h>
#include <pxr/base/gf/vec3f.h>
#include <pxr/base/gf/quath.h>
#include <pxr/base/vt/array.h>
#include <pxr/base/tf/pathUtils.h>
#include <pxr/base/tf/diagnostic.h>
#include <pxr/usd/sdf/layer.h>
#include <pxr/usd/usd/references.h>
#include <pxr/usd/usdGeom/metrics.h>
#include <pxr/usd/ar/resolver.h>
#include <pxr/usd/ar/resolverContextBinder.h>
#include <pxr/usd/usd/modelAPI.h>
#include <pxr/usd/kind/registry.h>
#include <pxr/usd/usdGeom/imageable.h>
#include <pxr/usd/usdGeom/tokens.h>
#include <pxr/usd/sdf/layerUtils.h>

#include <vector>
#include <string>
#include <memory>
#include <iostream>
#include <set>
#include <algorithm>
#include <chrono>
#include <cfloat>

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
    std::vector<uint32_t> face_material_ids;  // Material index per triangle (for GeomSubsets)
    GfMatrix4d transform;

    // UV seam split tracking for vertex animation
    // Maps split vertex index -> original USD vertex index
    // Used to expand animated vertices to match split mesh
    std::vector<uint32_t> vertex_index_map;
    bool has_uv_split = false;

    // Purpose attribute (default/render/proxy/guide)
    int purpose = 0;  // 0=default, 1=render, 2=proxy, 3=guide

    // Original polygon topology (for subdivision surfaces)
    std::vector<int32_t> face_vertex_counts_orig;
    std::vector<int32_t> face_vertex_indices_orig;

    // Crease data (for subdivision surfaces)
    std::vector<int32_t> crease_indices;
    std::vector<int32_t> crease_lengths;
    std::vector<float> crease_sharpnesses;

    // True if this mesh came from a native instance proxy
    bool is_instance_proxy = false;

    // Computed visibility (inherited)
    bool visible = true;

    // Double-sided flag
    bool double_sided = false;

    // Subdivision scheme
    std::string subdivision_scheme = "none";

    // Normals interpolation (0=vertex, 1=faceVarying, 2=uniform, 3=constant)
    int normals_interpolation = 0;

    // Display color (primvars:displayColor — RGB, per-vertex or single)
    std::vector<float> display_color;

    // Display opacity (primvars:displayOpacity — single value)
    float display_opacity = 1.0f;

    // True if xformOpOrder contains !resetXformStack!
    bool resets_xform_stack = false;

    // Material path resolved during traversal (needed for instance proxies
    // whose virtual paths fail GetPrimAtPath() after traversal)
    std::string bound_material_path;
};

/// Native instance: references a prototype mesh with a unique world transform
struct CachedNativeInstance {
    int prototype_mesh_index;
    GfMatrix4d world_transform;
    int material_override_index;  // -1 = use prototype material
};

/// Cached instancer data for FFI transfer
struct CachedInstancer {
    std::string path;
    std::vector<std::string> prototype_paths;
    std::vector<const char*> prototype_path_ptrs;  // For C API
    std::vector<float> transforms;
    std::vector<int32_t> proto_indices;
    std::vector<float> velocities;         // vec3 per instance
    std::vector<float> angular_velocities; // vec3 per instance
    std::vector<int64_t> invisible_ids;    // IDs of invisible instances
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
    bool visible = true;  // Computed inherited visibility
};

/// Cached animation sample for a single time
struct CachedXformSample {
    double time;
    float transform[16];
};

/// Cached mesh animation data
struct CachedMeshAnimation {
    std::vector<CachedXformSample> xform_samples;
};

/// Cached vertex animation info for a mesh
struct CachedVertexAnimation {
    bool has_animated_vertices = false;
    std::vector<double> time_samples;
};

/// Cached instancer animation data
struct CachedInstancerAnimation {
    std::vector<double> time_samples;
    size_t instance_count;
    std::vector<float> transforms;  // Flattened: time_sample_count * instance_count * 16
};

/// Cached camera animation data
struct CachedCameraAnimation {
    std::string path;
    std::vector<CachedXformSample> xform_samples;
};

/// Cached material data for FFI transfer (UsdPreviewSurface or MaterialX)
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
    bool is_materialx;  // True if material is from MaterialX, false for UsdPreviewSurface
};

/// Cached light data for FFI transfer (UsdLux)
struct CachedLight {
    std::string path;
    UsdBridgeLightType type;
    float color[3];
    float intensity;
    float exposure;
    float transform[16];
    float angle;   // DistantLight
    float radius;  // SphereLight/DiskLight
    float width;   // RectLight
    float height;  // RectLight
    std::string texture_path;  // DomeLight
    float length;  // CylinderLight

    // ShapingAPI
    float shaping_cone_angle = 0.0f;
    float shaping_cone_softness = 0.0f;
    float shaping_focus = 0.0f;
    std::string shaping_ies_file;
};

/// Cached UsdGeomPoints data for FFI transfer
struct CachedPoints {
    std::string path;
    std::vector<float> positions;  // xyz triplets
    std::vector<float> widths;
    std::vector<float> normals;    // xyz triplets
    std::vector<int64_t> ids;
    float transform[16];
};

/// Cached primvar data for a mesh
struct CachedPrimvar {
    std::string name;
    UsdBridgePrimvarType type;
    UsdBridgePrimvarInterpolation interpolation;
    std::vector<float> float_data;
    std::vector<int32_t> int_data;
    size_t element_count = 0;
};

/// Internal stage representation
struct UsdBridgeStage {
    UsdStageRefPtr stage;
    std::vector<CachedMesh> meshes;
    std::vector<CachedInstancer> instancers;
    std::vector<CachedMaterial> materials;
    std::vector<CachedLight> lights;
    std::vector<CachedNativeInstance> native_instances;
    std::vector<std::string> mesh_material_paths;  // Material path per mesh
    std::vector<CachedPrimInfo> all_prims;  // All prims in traversal order
    std::vector<std::string> root_paths;    // Direct children of pseudo-root
    std::vector<const char*> root_path_ptrs;
    std::vector<CachedPoints> points_prims;
    std::vector<std::vector<CachedPrimvar>> mesh_primvars;  // Per-mesh primvars
    bool cached;
    bool prims_cached;
    bool materials_cached;
    bool lights_cached;
    bool points_cached;
    bool animation_cached;

    // Animation caches
    std::vector<CachedMeshAnimation> mesh_animations;
    std::vector<CachedInstancerAnimation> instancer_animations;
    std::vector<CachedCameraAnimation> camera_animations;
    std::vector<CachedVertexAnimation> vertex_animations;
    bool vertex_animation_cached;

    UsdBridgeStage() : cached(false), prims_cached(false), materials_cached(false), lights_cached(false), points_cached(false), animation_cached(false), vertex_animation_cached(false) {}

    ~UsdBridgeStage() {
        // Clear cached data to ensure proper cleanup
        meshes.clear();
        instancers.clear();
        materials.clear();
        lights.clear();
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
/// Also outputs original face index for each triangle (for material mapping)
static void triangulate_mesh(
    const VtArray<int>& face_vertex_counts,
    const VtArray<int>& face_vertex_indices,
    std::vector<uint32_t>& out_indices,
    std::vector<uint32_t>& out_triangle_face_indices
) {
    out_indices.clear();
    out_triangle_face_indices.clear();
    size_t idx_offset = 0;

    for (size_t face_idx = 0; face_idx < face_vertex_counts.size(); ++face_idx) {
        int face_size = face_vertex_counts[face_idx];
        if (face_size < 3) {
            idx_offset += face_size;
            continue;
        }

        // Fan triangulation: (0,1,2), (0,2,3), (0,3,4), ...
        for (int i = 1; i < face_size - 1; ++i) {
            out_indices.push_back(static_cast<uint32_t>(face_vertex_indices[idx_offset]));
            out_indices.push_back(static_cast<uint32_t>(face_vertex_indices[idx_offset + i]));
            out_indices.push_back(static_cast<uint32_t>(face_vertex_indices[idx_offset + i + 1]));
            out_triangle_face_indices.push_back(static_cast<uint32_t>(face_idx));
        }
        idx_offset += face_size;
    }
}

/// Flat-copy GfMatrix4d (row-major) to float[16].
/// Rust side calls from_cols_array() on this data, which implicitly transposes
/// from USD's row-vector convention to glam's column-vector convention.
static void matrix_to_float16(const GfMatrix4d& mat, float* out) {
    const double* data = mat.GetArray();
    for (int i = 0; i < 16; ++i) {
        out[i] = static_cast<float>(data[i]);
    }
}

// Forward declaration - materials must be cached before meshes for GeomSubset support
static void cache_material_data(UsdBridgeStage* bridge);

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

    // Traverse all prims in depth-first order (including instance proxies)
    for (const UsdPrim& prim : bridge->stage->Traverse(
            UsdTraverseInstanceProxies(UsdPrimDefaultPredicate))) {
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

        // Compute inherited visibility
        UsdGeomImageable imageable(prim);
        if (imageable) {
            info.visible = (imageable.ComputeVisibility() != UsdGeomTokens->invisible);
        }

        bridge->all_prims.push_back(std::move(info));
    }

    bridge->prims_cached = true;
}

/// Cache all mesh and instancer data from the stage
static void cache_stage_data(UsdBridgeStage* bridge) {
    if (bridge->cached) return;

    using namespace std::chrono;
    auto func_start = high_resolution_clock::now();

    // Cache materials first - needed for GeomSubset material assignment
    auto mat_start = high_resolution_clock::now();
    cache_material_data(bridge);
    auto mat_time = duration_cast<milliseconds>(high_resolution_clock::now() - mat_start).count();

    // Timing accumulators for mesh processing
    long long time_vertices = 0, time_triangulate = 0, time_subsets = 0;
    long long time_normals = 0, time_uvs = 0, time_transform = 0;
    size_t total_verts = 0, total_tris = 0;

    UsdGeomXformCache xform_cache;

    // Track seen mesh paths to avoid duplicates.
    // For instance proxies, key on prototype path so geometry is cached once.
    // Maps dedup_path -> index in bridge->meshes.
    std::map<std::string, int> prototype_mesh_index;

    // Traverse all prims (including instance proxies for native instancing)
    for (const UsdPrim& prim : bridge->stage->Traverse(
            UsdTraverseInstanceProxies(UsdPrimDefaultPredicate))) {
        // Check for UsdGeomMesh
        if (prim.IsA<UsdGeomMesh>()) {
            std::string prim_path = prim.GetPath().GetString();

            // For instance proxies, dedup by prototype path (shared geometry).
            // For regular meshes, dedup by scene path (same as before).
            std::string dedup_path = prim_path;
            bool is_proxy = prim.IsInstanceProxy();
            if (is_proxy) {
                UsdPrim proto_prim = prim.GetPrimInPrototype();
                if (proto_prim) {
                    dedup_path = proto_prim.GetPath().GetString();
                }
            }

            // If geometry already cached, add as native instance
            auto proto_it = prototype_mesh_index.find(dedup_path);
            if (proto_it != prototype_mesh_index.end()) {
                CachedNativeInstance inst;
                inst.prototype_mesh_index = proto_it->second;
                inst.world_transform = xform_cache.GetLocalToWorldTransform(prim);
                inst.material_override_index = -1;

                // Check for material override vs prototype
                UsdShadeMaterialBindingAPI binding_api(prim);
                UsdShadeMaterial bound_material = binding_api.ComputeBoundMaterial();
                if (bound_material) {
                    std::string mat_path = bound_material.GetPath().GetString();
                    // Look up material index
                    for (size_t mi = 0; mi < bridge->materials.size(); ++mi) {
                        if (bridge->materials[mi].path == mat_path) {
                            inst.material_override_index = static_cast<int>(mi);
                            break;
                        }
                    }
                }

                bridge->native_instances.push_back(inst);
                continue;
            }

            UsdGeomMesh mesh(prim);
            CachedMesh cached;
            cached.path = prim.GetPath().GetString();

            // Get points at first time sample for animated geometry
            // Use stage's startTimeCode if available, otherwise first authored sample
            VtArray<GfVec3f> points;
            UsdTimeCode timeCode = UsdTimeCode::EarliestTime();

            auto vert_start = high_resolution_clock::now();

            // Check if points have time samples - if so, use startTimeCode
            UsdAttribute pointsAttr = mesh.GetPointsAttr();
            std::vector<double> pointTimeSamples;
            if (pointsAttr.GetTimeSamples(&pointTimeSamples) && !pointTimeSamples.empty()) {
                // Use stage's startTimeCode or first sample time
                double startTime = bridge->stage->GetStartTimeCode();
                if (startTime >= pointTimeSamples.front() && startTime <= pointTimeSamples.back()) {
                    timeCode = UsdTimeCode(startTime);
                } else {
                    timeCode = UsdTimeCode(pointTimeSamples.front());
                }
            }

            mesh.GetPointsAttr().Get(&points, timeCode);

            // Pre-allocate to exact size to minimize memory overhead
            cached.vertices.reserve(points.size() * 3);
            cached.vertices.shrink_to_fit();
            for (const auto& p : points) {
                cached.vertices.push_back(p[0]);
                cached.vertices.push_back(p[1]);
                cached.vertices.push_back(p[2]);
            }
            time_vertices += duration_cast<milliseconds>(high_resolution_clock::now() - vert_start).count();
            total_verts += points.size();

            // Read subdivision scheme early (needed to decide whether to store polygon topology)
            {
                TfToken subdivScheme;
                if (mesh.GetSubdivisionSchemeAttr().Get(&subdivScheme)) {
                    cached.subdivision_scheme = subdivScheme.GetString();
                }
            }

            // Get face topology and triangulate (use same timeCode as points)
            auto tri_start = high_resolution_clock::now();
            VtArray<int> face_vertex_counts;
            VtArray<int> face_vertex_indices;
            mesh.GetFaceVertexCountsAttr().Get(&face_vertex_counts, timeCode);
            mesh.GetFaceVertexIndicesAttr().Get(&face_vertex_indices, timeCode);

            // Store original polygon topology for subdivision surfaces
            bool is_subd = (cached.subdivision_scheme == "catmullClark" || cached.subdivision_scheme == "loop");
            if (is_subd) {
                cached.face_vertex_counts_orig.assign(face_vertex_counts.begin(), face_vertex_counts.end());
                cached.face_vertex_indices_orig.assign(face_vertex_indices.begin(), face_vertex_indices.end());

                // Read crease data
                VtArray<int> creaseIndices, creaseLengths;
                VtArray<float> creaseSharpnesses;
                mesh.GetCreaseIndicesAttr().Get(&creaseIndices);
                mesh.GetCreaseLengthsAttr().Get(&creaseLengths);
                mesh.GetCreaseSharpnessesAttr().Get(&creaseSharpnesses);
                if (!creaseIndices.empty()) {
                    cached.crease_indices.assign(creaseIndices.begin(), creaseIndices.end());
                    cached.crease_lengths.assign(creaseLengths.begin(), creaseLengths.end());
                    cached.crease_sharpnesses.assign(creaseSharpnesses.begin(), creaseSharpnesses.end());
                }
            }

            std::vector<uint32_t> triangle_face_indices;
            triangulate_mesh(face_vertex_counts, face_vertex_indices, cached.indices, triangle_face_indices);
            time_triangulate += duration_cast<milliseconds>(high_resolution_clock::now() - tri_start).count();
            total_tris += cached.indices.size() / 3;

            // Extract GeomSubsets for per-face material assignment
            auto subset_start = high_resolution_clock::now();
            size_t num_faces = face_vertex_counts.size();

            std::vector<UsdGeomSubset> subsets = UsdGeomSubset::GetAllGeomSubsets(mesh);

            if (!subsets.empty()) {
                // Build material path -> index map
                std::map<std::string, uint32_t> material_path_to_index;
                for (size_t i = 0; i < bridge->materials.size(); ++i) {
                    material_path_to_index[bridge->materials[i].path] = static_cast<uint32_t>(i);
                }

                std::vector<uint32_t> face_material_map(num_faces, 0);

                for (const auto& subset : subsets) {
                    // Get material binding for this subset
                    UsdShadeMaterialBindingAPI binding_api(subset.GetPrim());
                    UsdShadeMaterial bound_material = binding_api.ComputeBoundMaterial();

                    // Skip subsets without valid material bindings (e.g., __subdivs__ from Houdini)
                    if (!bound_material) {
                        continue;
                    }

                    std::string mat_path = bound_material.GetPath().GetString();
                    auto it = material_path_to_index.find(mat_path);
                    if (it == material_path_to_index.end()) {
                        continue;
                    }
                    uint32_t material_idx = it->second;

                    // Get face indices for this subset
                    VtArray<int> subset_indices;
                    subset.GetIndicesAttr().Get(&subset_indices);

                    // Assign material to these faces
                    for (int face_idx : subset_indices) {
                        if (face_idx >= 0 && static_cast<size_t>(face_idx) < num_faces) {
                            face_material_map[face_idx] = material_idx;
                        }
                    }
                }

                // Map per-original-face materials to per-triangle
                cached.face_material_ids.reserve(triangle_face_indices.size());
                for (uint32_t orig_face : triangle_face_indices) {
                    cached.face_material_ids.push_back(face_material_map[orig_face]);
                }
            }
            // When no GeomSubsets, leave face_material_ids empty.
            // The shader will use the per-instance material binding instead.
            time_subsets += duration_cast<milliseconds>(high_resolution_clock::now() - subset_start).count();

            // Get normals (optional) - track interpolation for UV seam split
            auto normal_start = high_resolution_clock::now();
            VtArray<GfVec3f> normals;
            TfToken normalsInterpolation;
            if (mesh.GetNormalsAttr().Get(&normals, timeCode)) {
                normalsInterpolation = mesh.GetNormalsInterpolation();
                cached.normals.reserve(normals.size() * 3);
                for (const auto& n : normals) {
                    cached.normals.push_back(n[0]);
                    cached.normals.push_back(n[1]);
                    cached.normals.push_back(n[2]);
                }

                // Store normals interpolation enum
                if (normalsInterpolation == UsdGeomTokens->faceVarying) {
                    cached.normals_interpolation = 1;
                } else if (normalsInterpolation == UsdGeomTokens->uniform) {
                    cached.normals_interpolation = 2;
                } else if (normalsInterpolation == UsdGeomTokens->constant) {
                    cached.normals_interpolation = 3;
                } else {
                    cached.normals_interpolation = 0; // vertex (default)
                }
            }
            time_normals += duration_cast<milliseconds>(high_resolution_clock::now() - normal_start).count();

            // Get UV coordinates from primvars:st (optional)
            auto uv_start = high_resolution_clock::now();
            UsdGeomPrimvarsAPI primvarsAPI(mesh);
            UsdGeomPrimvar stPrimvar = primvarsAPI.GetPrimvar(TfToken("st"));
            if (stPrimvar) {
                TfToken interpolation = stPrimvar.GetInterpolation();
                VtArray<GfVec2f> uvs;
                VtIntArray uvIndices;
                bool hasIndices = stPrimvar.GetIndices(&uvIndices, timeCode);

                if (stPrimvar.Get(&uvs, timeCode)) {
                    if (interpolation == UsdGeomTokens->faceVarying) {
                        // faceVarying: one UV per face-vertex. Split vertices at UV seams.
                        // Map (original_vertex, uv) -> new_vertex_index
                        std::map<std::pair<int, std::pair<int,int>>, uint32_t> vertUvToNew;
                        std::vector<float> newVertices;
                        std::vector<float> newNormals;
                        std::vector<float> newUvs;
                        std::vector<uint32_t> newIndices;
                        std::vector<uint32_t> newVertexIndexMap;  // Maps split vertex -> original USD vertex

                        newVertices.reserve(cached.vertices.size());
                        newNormals.reserve(cached.normals.size());
                        newUvs.reserve(face_vertex_indices.size() * 2);
                        newIndices.reserve(cached.indices.size());
                        newVertexIndexMap.reserve(cached.vertices.size() / 3);

                        // Rebuild triangulated indices with UV-split vertices
                        size_t faceVertIdx = 0;
                        for (size_t faceIdx = 0; faceIdx < face_vertex_counts.size(); ++faceIdx) {
                            int faceSize = face_vertex_counts[faceIdx];
                            if (faceSize < 3) {
                                faceVertIdx += faceSize;
                                continue;
                            }

                            // Fan triangulation matching triangulate_mesh()
                            for (int i = 1; i < faceSize - 1; ++i) {
                                int localIndices[3] = {0, i, i + 1};
                                for (int li = 0; li < 3; ++li) {
                                    size_t fvIdx = faceVertIdx + localIndices[li];
                                    int origVert = face_vertex_indices[fvIdx];

                                    // Get UV for this face-vertex
                                    GfVec2f uv(0, 0);
                                    if (hasIndices && fvIdx < uvIndices.size()) {
                                        int uvIdx = uvIndices[fvIdx];
                                        if (uvIdx >= 0 && static_cast<size_t>(uvIdx) < uvs.size()) {
                                            uv = uvs[uvIdx];
                                        }
                                    } else if (fvIdx < uvs.size()) {
                                        uv = uvs[fvIdx];
                                    }

                                    // Quantize UV to detect "same" UVs (avoid float comparison issues)
                                    int uvKeyU = static_cast<int>(uv[0] * 10000);
                                    int uvKeyV = static_cast<int>(uv[1] * 10000);
                                    auto key = std::make_pair(origVert, std::make_pair(uvKeyU, uvKeyV));

                                    auto it = vertUvToNew.find(key);
                                    if (it != vertUvToNew.end()) {
                                        // Reuse existing vertex
                                        newIndices.push_back(it->second);
                                    } else {
                                        // Create new vertex
                                        uint32_t newIdx = static_cast<uint32_t>(newVertices.size() / 3);
                                        vertUvToNew[key] = newIdx;

                                        // Track original vertex for animation
                                        newVertexIndexMap.push_back(static_cast<uint32_t>(origVert >= 0 ? origVert : 0));

                                        // Copy position
                                        if (origVert >= 0 && static_cast<size_t>(origVert) < points.size()) {
                                            newVertices.push_back(points[origVert][0]);
                                            newVertices.push_back(points[origVert][1]);
                                            newVertices.push_back(points[origVert][2]);
                                        } else {
                                            newVertices.push_back(0); newVertices.push_back(0); newVertices.push_back(0);
                                        }

                                        // Copy normal if available - handle faceVarying vs vertex interpolation
                                        if (!normals.empty()) {
                                            GfVec3f normal(0, 1, 0);
                                            if (normalsInterpolation == UsdGeomTokens->faceVarying) {
                                                // faceVarying: index by face-vertex position
                                                if (fvIdx < normals.size()) {
                                                    normal = normals[fvIdx];
                                                }
                                            } else {
                                                // vertex interpolation: index by vertex
                                                if (origVert >= 0 && static_cast<size_t>(origVert) < normals.size()) {
                                                    normal = normals[origVert];
                                                }
                                            }
                                            newNormals.push_back(normal[0]);
                                            newNormals.push_back(normal[1]);
                                            newNormals.push_back(normal[2]);
                                        }

                                        // Store UV
                                        newUvs.push_back(uv[0]);
                                        newUvs.push_back(uv[1]);

                                        newIndices.push_back(newIdx);
                                    }
                                }
                            }
                            faceVertIdx += faceSize;
                        }

                        // Replace cached data with UV-split version
                        cached.vertices = std::move(newVertices);
                        cached.normals = std::move(newNormals);
                        cached.uvs = std::move(newUvs);
                        cached.indices = std::move(newIndices);
                        cached.vertex_index_map = std::move(newVertexIndexMap);
                        cached.has_uv_split = true;

                        // Rebuild face_material_ids for new triangle count
                        // (triangulate_mesh output is no longer valid, but we re-triangulated above)
                        // The triangle order matches, so face_material_ids should still be correct
                    } else {
                        // vertex or constant interpolation: direct mapping
                        cached.uvs.reserve(uvs.size() * 2);
                        for (const auto& uv : uvs) {
                            cached.uvs.push_back(uv[0]);
                            cached.uvs.push_back(uv[1]);
                        }
                    }
                }
            }
            time_uvs += duration_cast<milliseconds>(high_resolution_clock::now() - uv_start).count();

            // Get world transform
            auto xform_start = high_resolution_clock::now();
            cached.transform = xform_cache.GetLocalToWorldTransform(prim);
            time_transform += duration_cast<milliseconds>(high_resolution_clock::now() - xform_start).count();

            // Compute inherited purpose (not just directly-authored)
            UsdGeomImageable imageable(prim);
            TfToken purposeToken = imageable.ComputePurpose();
            if (purposeToken == UsdGeomTokens->render) cached.purpose = 1;
            else if (purposeToken == UsdGeomTokens->proxy) cached.purpose = 2;
            else if (purposeToken == UsdGeomTokens->guide) cached.purpose = 3;

            cached.is_instance_proxy = is_proxy;

            // Computed visibility (considers ancestor visibility)
            cached.visible = (imageable.ComputeVisibility() != UsdGeomTokens->invisible);

            // Double-sided flag
            {
                bool ds = false;
                if (mesh.GetDoubleSidedAttr().Get(&ds)) {
                    cached.double_sided = ds;
                }
            }

            // (subdivision scheme already read above, before face topology)

            // Display color (primvars:displayColor — fallback when no material)
            {
                UsdGeomPrimvar displayColorPv = primvarsAPI.GetPrimvar(TfToken("displayColor"));
                if (displayColorPv) {
                    VtArray<GfVec3f> colors;
                    if (displayColorPv.Get(&colors, timeCode) && !colors.empty()) {
                        cached.display_color.reserve(colors.size() * 3);
                        for (const auto& c : colors) {
                            cached.display_color.push_back(c[0]);
                            cached.display_color.push_back(c[1]);
                            cached.display_color.push_back(c[2]);
                        }
                    }
                }
            }

            // Display opacity (primvars:displayOpacity)
            {
                UsdGeomPrimvar displayOpacityPv = primvarsAPI.GetPrimvar(TfToken("displayOpacity"));
                if (displayOpacityPv) {
                    VtArray<float> opacities;
                    if (displayOpacityPv.Get(&opacities, timeCode) && !opacities.empty()) {
                        cached.display_opacity = opacities[0];
                    }
                }
            }

            // Check for !resetXformStack! in xformOpOrder
            {
                UsdGeomXformable xformable(prim);
                bool resetsXform = false;
                xformable.GetOrderedXformOps(&resetsXform);
                cached.resets_xform_stack = resetsXform;
            }

            // Resolve material binding while we have the live prim
            // (instance proxy paths are virtual — GetPrimAtPath() fails afterward)
            {
                UsdShadeMaterialBindingAPI binding_api(prim);
                UsdShadeMaterial bound_material = binding_api.ComputeBoundMaterial();
                if (bound_material) {
                    cached.bound_material_path = bound_material.GetPath().GetString();
                }
            }

            // Track mesh index for native instance dedup
            int mesh_idx = static_cast<int>(bridge->meshes.size());
            prototype_mesh_index[dedup_path] = mesh_idx;

            bridge->meshes.push_back(std::move(cached));
        }

        // Check for UsdGeomPointInstancer (skip instance proxies — they reference prototype instancers)
        if (prim.IsA<UsdGeomPointInstancer>() && !prim.IsInstanceProxy()) {
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

            // Velocities (for motion blur interpolation)
            {
                VtArray<GfVec3f> vels;
                if (instancer.GetVelocitiesAttr().Get(&vels, UsdTimeCode::Default()) && !vels.empty()) {
                    cached.velocities.reserve(vels.size() * 3);
                    for (const auto& v : vels) {
                        cached.velocities.push_back(v[0]);
                        cached.velocities.push_back(v[1]);
                        cached.velocities.push_back(v[2]);
                    }
                }
            }

            // Angular velocities (for motion blur)
            {
                VtArray<GfVec3f> angVels;
                if (instancer.GetAngularVelocitiesAttr().Get(&angVels, UsdTimeCode::Default()) && !angVels.empty()) {
                    cached.angular_velocities.reserve(angVels.size() * 3);
                    for (const auto& v : angVels) {
                        cached.angular_velocities.push_back(v[0]);
                        cached.angular_velocities.push_back(v[1]);
                        cached.angular_velocities.push_back(v[2]);
                    }
                }
            }

            // Invisible instance IDs
            {
                VtArray<int64_t> invisIds;
                if (instancer.GetInvisibleIdsAttr().Get(&invisIds) && !invisIds.empty()) {
                    cached.invisible_ids.assign(invisIds.begin(), invisIds.end());
                }
            }

            bridge->instancers.push_back(std::move(cached));
        }
    }

    // Populate mesh-to-material bindings from pre-resolved paths
    // (resolved during traversal when the live prim was available)
    bridge->mesh_material_paths.clear();
    for (const auto& mesh : bridge->meshes) {
        bridge->mesh_material_paths.push_back(mesh.bound_material_path);
    }

    // Print timing breakdown
    auto total_time = duration_cast<milliseconds>(high_resolution_clock::now() - func_start).count();
    std::cout << "[USD_BRIDGE] cache_stage_data breakdown:" << std::endl;
    std::cout << "[USD_BRIDGE]   Materials:    " << mat_time << "ms" << std::endl;
    std::cout << "[USD_BRIDGE]   Vertices:     " << time_vertices << "ms (" << total_verts << " verts)" << std::endl;
    std::cout << "[USD_BRIDGE]   Triangulate:  " << time_triangulate << "ms (" << total_tris << " tris)" << std::endl;
    std::cout << "[USD_BRIDGE]   GeomSubsets:  " << time_subsets << "ms" << std::endl;
    std::cout << "[USD_BRIDGE]   Normals:      " << time_normals << "ms" << std::endl;
    std::cout << "[USD_BRIDGE]   UVs:          " << time_uvs << "ms" << std::endl;
    std::cout << "[USD_BRIDGE]   Transforms:   " << time_transform << "ms" << std::endl;
    std::cout << "[USD_BRIDGE]   Native inst:  " << bridge->native_instances.size() << std::endl;
    std::cout << "[USD_BRIDGE]   SUBTOTAL:     " << total_time << "ms" << std::endl;

    bridge->cached = true;
}

/// Resolve an SdfAssetPath, anchoring relative paths against the shader's
/// source layer so that paths like "../../texture/foo.<UDIM>.jpg" resolve
/// correctly even when the material lives in a sublayer/reference.
static std::string resolve_asset_path(const SdfAssetPath& asset_path,
                                       const UsdPrim& shader_prim) {
    // Prefer the pre-resolved absolute path (works for non-UDIM textures)
    if (!asset_path.GetResolvedPath().empty()) {
        return asset_path.GetResolvedPath();
    }

    std::string raw = asset_path.GetAssetPath();
    if (raw.empty()) return "";

    // For relative paths (including UDIM templates), anchor against the
    // USD layer that defines this shader prim.
    UsdPrim lookup_prim = shader_prim.IsInstanceProxy()
        ? shader_prim.GetPrimInPrototype()
        : shader_prim;

    auto prim_stack = lookup_prim.GetPrimStack();
    if (!prim_stack.empty()) {
        SdfLayerHandle source_layer = prim_stack[0]->GetLayer();
        if (source_layer) {
            return SdfComputeAssetPathRelativeToLayer(source_layer, raw);
        }
    }

    return raw;
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
                    return resolve_asset_path(asset_path, shader_prim);
                }
            }
        }
    }
    return "";
}

/// Helper to check if a shader ID is a MaterialX standard_surface
static bool is_materialx_standard_surface(const TfToken& shader_id) {
    std::string id_str = shader_id.GetString();
    // MaterialX standard_surface shaders typically have IDs like:
    // ND_standard_surface_surfaceshader
    // ND_standard_surface_to_UsdPreviewSurface (converted)
    return id_str.find("ND_standard_surface") != std::string::npos ||
           id_str.find("standard_surface") != std::string::npos;
}

/// Helper to extract texture path from MaterialX ND_image_* nodes
static std::string get_materialx_texture_path(const UsdShadeInput& input) {
    if (!input) return "";

    SdfPathVector connections;
    input.GetRawConnectedSourcePaths(&connections);

    for (const auto& conn_path : connections) {
        SdfPath prim_path = conn_path.GetPrimPath();
        UsdPrim shader_prim = input.GetPrim().GetStage()->GetPrimAtPath(prim_path);
        if (!shader_prim) continue;

        UsdShadeShader shader(shader_prim);
        if (!shader) continue;

        TfToken shader_id;
        shader.GetIdAttr().Get(&shader_id);
        std::string id_str = shader_id.GetString();

        // Check for MaterialX image nodes: ND_image_color3, ND_image_float, etc.
        if (id_str.find("ND_image") != std::string::npos ||
            id_str.find("image") != std::string::npos) {
            // MaterialX uses "file" attribute for texture path
            UsdShadeInput file_input = shader.GetInput(TfToken("file"));
            if (file_input) {
                SdfAssetPath asset_path;
                if (file_input.Get(&asset_path)) {
                    return resolve_asset_path(asset_path, shader_prim);
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

    // Dedup set: instance proxies visit the same material once per instance.
    // Key on prototype path so identical materials are cached only once.
    std::set<std::string> seen_material_paths;

    // Find all UsdShadeMaterial prims (including inside instance prototypes)
    for (const UsdPrim& prim : bridge->stage->Traverse(
            UsdTraverseInstanceProxies(UsdPrimDefaultPredicate))) {
        if (!prim.IsA<UsdShadeMaterial>()) continue;

        std::string mat_path = prim.GetPath().GetString();

        // For instance proxies, dedup by prototype path (shared material)
        std::string dedup_key = mat_path;
        if (prim.IsInstanceProxy()) {
            UsdPrim proto_prim = prim.GetPrimInPrototype();
            if (proto_prim) {
                dedup_key = proto_prim.GetPath().GetString();
            }
        }

        if (seen_material_paths.count(dedup_key) > 0) continue;
        seen_material_paths.insert(dedup_key);

        UsdShadeMaterial material(prim);
        CachedMaterial cached;
        cached.path = mat_path;

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
        cached.is_materialx = false;

        // First, try to find MaterialX shader by looking for mtlx:surface output
        // or by searching material children for mtlxstandard_surface
        UsdShadeShader mtlx_shader;

        // Check for mtlx:surface output (MaterialX-specific)
        UsdShadeOutput mtlx_surface = material.GetOutput(TfToken("mtlx:surface"));
        if (mtlx_surface) {
            SdfPathVector mtlx_connections;
            mtlx_surface.GetRawConnectedSourcePaths(&mtlx_connections);
            if (!mtlx_connections.empty()) {
                UsdPrim mtlx_prim = bridge->stage->GetPrimAtPath(mtlx_connections[0].GetPrimPath());
                if (mtlx_prim) {
                    // Check if this is a NodeGraph (Karma materials) or a Shader
                    if (mtlx_prim.IsA<UsdShadeNodeGraph>()) {
                        // Search inside NodeGraph for standard_surface shader
                        for (const UsdPrim& ng_child : mtlx_prim.GetDescendants()) {
                            std::string child_name = ng_child.GetName().GetString();
                            if (child_name.find("mtlxstandard_surface") != std::string::npos ||
                                child_name.find("standard_surface") != std::string::npos) {
                                UsdShadeShader potential_shader(ng_child);
                                if (potential_shader) {
                                    mtlx_shader = potential_shader;
                                    break;
                                }
                            }
                        }
                    } else {
                        mtlx_shader = UsdShadeShader(mtlx_prim);
                    }
                }
            }
        }

        // If no mtlx:surface, search children for mtlxstandard_surface
        if (!mtlx_shader) {
            for (const UsdPrim& child : prim.GetChildren()) {
                std::string child_name = child.GetName().GetString();
                if (child_name.find("mtlxstandard_surface") != std::string::npos ||
                    child_name.find("standard_surface") != std::string::npos) {
                    UsdShadeShader potential_shader(child);
                    if (potential_shader) {
                        mtlx_shader = potential_shader;
                        break;
                    }
                }
            }
        }

        // If we found a MaterialX shader, use it
        if (mtlx_shader) {
            cached.is_materialx = true;
            UsdShadeInput input;

            // MaterialX standard_surface parameter names
            input = mtlx_shader.GetInput(TfToken("base_color"));
            if (input) {
                GfVec3f color;
                if (input.Get(&color)) {
                    cached.diffuse_color[0] = color[0];
                    cached.diffuse_color[1] = color[1];
                    cached.diffuse_color[2] = color[2];
                }
                cached.diffuse_texture = get_materialx_texture_path(input);
            }

            input = mtlx_shader.GetInput(TfToken("metalness"));
            if (input) {
                input.Get(&cached.metallic);
                cached.metallic_texture = get_materialx_texture_path(input);
            }

            input = mtlx_shader.GetInput(TfToken("specular_roughness"));
            if (input) {
                input.Get(&cached.roughness);
                cached.roughness_texture = get_materialx_texture_path(input);
            }

            input = mtlx_shader.GetInput(TfToken("specular"));
            if (input) {
                input.Get(&cached.specular);
            }

            input = mtlx_shader.GetInput(TfToken("opacity"));
            if (input) {
                GfVec3f opacity_vec;
                if (input.Get(&opacity_vec)) {
                    cached.opacity = (opacity_vec[0] + opacity_vec[1] + opacity_vec[2]) / 3.0f;
                } else {
                    float opacity_scalar;
                    if (input.Get(&opacity_scalar)) {
                        cached.opacity = opacity_scalar;
                    }
                }
            }

            input = mtlx_shader.GetInput(TfToken("emission_color"));
            if (input) {
                GfVec3f emissive;
                if (input.Get(&emissive)) {
                    float emission = 0.0f;
                    UsdShadeInput emission_input = mtlx_shader.GetInput(TfToken("emission"));
                    if (emission_input) {
                        emission_input.Get(&emission);
                    }
                    cached.emissive_color[0] = emissive[0] * emission;
                    cached.emissive_color[1] = emissive[1] * emission;
                    cached.emissive_color[2] = emissive[2] * emission;
                }
                cached.emissive_texture = get_materialx_texture_path(input);
            }

            input = mtlx_shader.GetInput(TfToken("normal"));
            if (input) {
                cached.normal_texture = get_materialx_texture_path(input);
            }

            bridge->materials.push_back(std::move(cached));
            continue;
        }

        // Fall back to standard surface output for UsdPreviewSurface
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

        // Get shader ID
        TfToken shader_id;
        shader.GetIdAttr().Get(&shader_id);

        // Check for MaterialX standard_surface first
        if (is_materialx_standard_surface(shader_id)) {
            cached.is_materialx = true;
            UsdShadeInput input;

            // MaterialX standard_surface uses different parameter names:
            // base_color (not diffuseColor)
            input = shader.GetInput(TfToken("base_color"));
            if (input) {
                GfVec3f color;
                if (input.Get(&color)) {
                    cached.diffuse_color[0] = color[0];
                    cached.diffuse_color[1] = color[1];
                    cached.diffuse_color[2] = color[2];
                }
                cached.diffuse_texture = get_materialx_texture_path(input);
            }

            // metalness (not metallic)
            input = shader.GetInput(TfToken("metalness"));
            if (input) {
                input.Get(&cached.metallic);
                cached.metallic_texture = get_materialx_texture_path(input);
            }

            // specular_roughness (not roughness)
            input = shader.GetInput(TfToken("specular_roughness"));
            if (input) {
                input.Get(&cached.roughness);
                cached.roughness_texture = get_materialx_texture_path(input);
            }

            // specular
            input = shader.GetInput(TfToken("specular"));
            if (input) {
                input.Get(&cached.specular);
            }

            // opacity
            input = shader.GetInput(TfToken("opacity"));
            if (input) {
                GfVec3f opacity_vec;
                if (input.Get(&opacity_vec)) {
                    // MaterialX uses vec3 for opacity, take average
                    cached.opacity = (opacity_vec[0] + opacity_vec[1] + opacity_vec[2]) / 3.0f;
                } else {
                    float opacity_scalar;
                    if (input.Get(&opacity_scalar)) {
                        cached.opacity = opacity_scalar;
                    }
                }
            }

            // emission_color + emission (multiply for intensity)
            input = shader.GetInput(TfToken("emission_color"));
            if (input) {
                GfVec3f emissive;
                if (input.Get(&emissive)) {
                    // Check emission intensity
                    float emission = 0.0f;
                    UsdShadeInput emission_input = shader.GetInput(TfToken("emission"));
                    if (emission_input) {
                        emission_input.Get(&emission);
                    }
                    cached.emissive_color[0] = emissive[0] * emission;
                    cached.emissive_color[1] = emissive[1] * emission;
                    cached.emissive_color[2] = emissive[2] * emission;
                }
                cached.emissive_texture = get_materialx_texture_path(input);
            }

            // normal (for normal mapping)
            input = shader.GetInput(TfToken("normal"));
            if (input) {
                cached.normal_texture = get_materialx_texture_path(input);
            }

            bridge->materials.push_back(std::move(cached));
            continue;
        }

        // Fall back to UsdPreviewSurface
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

    // Note: mesh_material_paths is populated in cache_stage_data() after meshes are cached
    bridge->materials_cached = true;
}

/// Cache all light data from the stage (UsdLux)
/// Cache UsdGeomPoints prims
static void cache_points_data(UsdBridgeStage* bridge) {
    if (bridge->points_cached) return;

    bridge->points_prims.clear();
    UsdGeomXformCache xform_cache;

    for (const UsdPrim& prim : bridge->stage->Traverse(
            UsdTraverseInstanceProxies(UsdPrimDefaultPredicate))) {
        if (!prim.IsA<UsdGeomPoints>()) continue;

        UsdGeomPoints pointsPrim(prim);
        CachedPoints cached;
        cached.path = prim.GetPath().GetString();

        // Positions
        VtArray<GfVec3f> positions;
        if (pointsPrim.GetPointsAttr().Get(&positions)) {
            cached.positions.reserve(positions.size() * 3);
            for (const auto& p : positions) {
                cached.positions.push_back(p[0]);
                cached.positions.push_back(p[1]);
                cached.positions.push_back(p[2]);
            }
        }

        // Widths
        VtArray<float> widths;
        if (pointsPrim.GetWidthsAttr().Get(&widths)) {
            cached.widths.assign(widths.begin(), widths.end());
        }

        // Normals
        VtArray<GfVec3f> normals;
        if (pointsPrim.GetNormalsAttr().Get(&normals)) {
            cached.normals.reserve(normals.size() * 3);
            for (const auto& n : normals) {
                cached.normals.push_back(n[0]);
                cached.normals.push_back(n[1]);
                cached.normals.push_back(n[2]);
            }
        }

        // IDs
        VtArray<int64_t> ids;
        if (pointsPrim.GetIdsAttr().Get(&ids)) {
            cached.ids.assign(ids.begin(), ids.end());
        }

        // Transform
        GfMatrix4d world_xform = xform_cache.GetLocalToWorldTransform(prim);
        matrix_to_float16(world_xform, cached.transform);

        bridge->points_prims.push_back(std::move(cached));
    }

    bridge->points_cached = true;
}

/// Cache arbitrary primvars for all meshes (excludes built-in st, normals, displayColor, displayOpacity)
static void cache_mesh_primvars(UsdBridgeStage* bridge) {
    // Must be called after cache_stage_data
    if (!bridge->cached) return;

    bridge->mesh_primvars.resize(bridge->meshes.size());

    static const std::set<std::string> built_in = {
        "st", "normals", "displayColor", "displayOpacity"
    };

    for (size_t mesh_idx = 0; mesh_idx < bridge->meshes.size(); ++mesh_idx) {
        const CachedMesh& mesh = bridge->meshes[mesh_idx];
        UsdPrim prim = bridge->stage->GetPrimAtPath(SdfPath(mesh.path));
        if (!prim) continue;

        UsdGeomPrimvarsAPI pvAPI(prim);
        std::vector<UsdGeomPrimvar> primvars = pvAPI.GetPrimvars();

        for (const auto& pv : primvars) {
            std::string name = pv.GetPrimvarName().GetString();
            if (built_in.count(name)) continue;

            TfToken interp = pv.GetInterpolation();
            SdfValueTypeName typeName = pv.GetTypeName();

            CachedPrimvar cached;
            cached.name = name;

            // Map interpolation
            if (interp == UsdGeomTokens->constant) cached.interpolation = USD_PRIMVAR_INTERP_CONSTANT;
            else if (interp == UsdGeomTokens->uniform) cached.interpolation = USD_PRIMVAR_INTERP_UNIFORM;
            else if (interp == UsdGeomTokens->vertex) cached.interpolation = USD_PRIMVAR_INTERP_VERTEX;
            else if (interp == UsdGeomTokens->faceVarying) cached.interpolation = USD_PRIMVAR_INTERP_FACE_VARYING;
            else cached.interpolation = USD_PRIMVAR_INTERP_VERTEX;

            // Extract data based on type
            if (typeName == SdfValueTypeNames->FloatArray || typeName == SdfValueTypeNames->Float) {
                VtArray<float> data;
                if (pv.Get(&data)) {
                    cached.type = USD_PRIMVAR_FLOAT;
                    cached.float_data.assign(data.begin(), data.end());
                    cached.element_count = data.size();
                }
            } else if (typeName == SdfValueTypeNames->Float2Array || typeName == SdfValueTypeNames->Float2) {
                VtArray<GfVec2f> data;
                if (pv.Get(&data)) {
                    cached.type = USD_PRIMVAR_FLOAT2;
                    cached.float_data.reserve(data.size() * 2);
                    for (const auto& v : data) {
                        cached.float_data.push_back(v[0]);
                        cached.float_data.push_back(v[1]);
                    }
                    cached.element_count = data.size();
                }
            } else if (typeName == SdfValueTypeNames->Float3Array || typeName == SdfValueTypeNames->Float3 ||
                       typeName == SdfValueTypeNames->Color3fArray || typeName == SdfValueTypeNames->Vector3fArray ||
                       typeName == SdfValueTypeNames->Normal3fArray || typeName == SdfValueTypeNames->Point3fArray) {
                VtArray<GfVec3f> data;
                if (pv.Get(&data)) {
                    cached.type = USD_PRIMVAR_FLOAT3;
                    cached.float_data.reserve(data.size() * 3);
                    for (const auto& v : data) {
                        cached.float_data.push_back(v[0]);
                        cached.float_data.push_back(v[1]);
                        cached.float_data.push_back(v[2]);
                    }
                    cached.element_count = data.size();
                }
            } else if (typeName == SdfValueTypeNames->IntArray || typeName == SdfValueTypeNames->Int) {
                VtArray<int> data;
                if (pv.Get(&data)) {
                    cached.type = USD_PRIMVAR_INT;
                    cached.int_data.assign(data.begin(), data.end());
                    cached.element_count = data.size();
                }
            } else {
                continue; // Skip unsupported types
            }

            if (cached.element_count > 0) {
                bridge->mesh_primvars[mesh_idx].push_back(std::move(cached));
            }
        }
    }
}

static void cache_light_data(UsdBridgeStage* bridge) {
    if (bridge->lights_cached) return;

    bridge->lights.clear();

    UsdGeomXformCache xform_cache;

    // Traverse all prims looking for lights (including instance proxies)
    for (const UsdPrim& prim : bridge->stage->Traverse(
            UsdTraverseInstanceProxies(UsdPrimDefaultPredicate))) {
        CachedLight light;
        bool is_light = false;

        if (prim.IsA<UsdLuxDistantLight>()) {
            UsdLuxDistantLight distant(prim);
            light.type = USD_LIGHT_DISTANT;
            is_light = true;

            // Get angle (angular diameter in degrees)
            float angle = 0.53f;  // Default: ~0.53 degrees (sun's angular diameter)
            distant.GetAngleAttr().Get(&angle);
            light.angle = angle;
            light.radius = 0.0f;
            light.width = 0.0f;
            light.height = 0.0f;
        }
        else if (prim.IsA<UsdLuxSphereLight>()) {
            UsdLuxSphereLight sphere(prim);
            light.type = USD_LIGHT_SPHERE;
            is_light = true;

            // Get radius
            float radius = 0.5f;
            sphere.GetRadiusAttr().Get(&radius);
            light.radius = radius;
            light.angle = 0.0f;
            light.width = 0.0f;
            light.height = 0.0f;
        }
        else if (prim.IsA<UsdLuxRectLight>()) {
            UsdLuxRectLight rect(prim);
            light.type = USD_LIGHT_RECT;
            is_light = true;

            // Get width and height
            float width = 1.0f, height = 1.0f;
            rect.GetWidthAttr().Get(&width);
            rect.GetHeightAttr().Get(&height);
            light.width = width;
            light.height = height;
            light.angle = 0.0f;
            light.radius = 0.0f;
        }
        else if (prim.IsA<UsdLuxDomeLight>()) {
            UsdLuxDomeLight dome(prim);
            light.type = USD_LIGHT_DOME;
            is_light = true;

            // Get texture file path
            SdfAssetPath texture_path;
            if (dome.GetTextureFileAttr().Get(&texture_path)) {
                light.texture_path = texture_path.GetResolvedPath().empty()
                    ? texture_path.GetAssetPath()
                    : texture_path.GetResolvedPath();
            }
            light.angle = 0.0f;
            light.radius = 0.0f;
            light.width = 0.0f;
            light.height = 0.0f;
        }
        else if (prim.IsA<UsdLuxCylinderLight>()) {
            UsdLuxCylinderLight cylinder(prim);
            light.type = USD_LIGHT_CYLINDER;
            is_light = true;

            float radius = 0.5f, length = 1.0f;
            cylinder.GetRadiusAttr().Get(&radius);
            cylinder.GetLengthAttr().Get(&length);
            light.radius = radius;
            light.length = length;
            light.angle = 0.0f;
            light.width = 0.0f;
            light.height = 0.0f;
        }
        else if (prim.IsA<UsdLuxDiskLight>()) {
            UsdLuxDiskLight disk(prim);
            light.type = USD_LIGHT_DISK;
            is_light = true;

            float radius = 0.5f;
            disk.GetRadiusAttr().Get(&radius);
            light.radius = radius;
            light.angle = 0.0f;
            light.length = 0.0f;
            light.width = 0.0f;
            light.height = 0.0f;
        }

        if (!is_light) continue;

        light.path = prim.GetPath().GetString();

        // Get common light attributes via UsdLuxLightAPI
        // Note: In USD 24+, lights inherit from UsdLuxBoundableLightBase or similar
        // We access attributes directly since all light types have these

        // Color (default white)
        GfVec3f color(1.0f, 1.0f, 1.0f);
        UsdAttribute colorAttr = prim.GetAttribute(TfToken("inputs:color"));
        if (colorAttr) colorAttr.Get(&color);
        light.color[0] = color[0];
        light.color[1] = color[1];
        light.color[2] = color[2];

        // Intensity (default 1.0)
        float intensity = 1.0f;
        UsdAttribute intensityAttr = prim.GetAttribute(TfToken("inputs:intensity"));
        if (intensityAttr) intensityAttr.Get(&intensity);
        light.intensity = intensity;

        // Exposure (default 0.0, multiplier is 2^exposure)
        float exposure = 0.0f;
        UsdAttribute exposureAttr = prim.GetAttribute(TfToken("inputs:exposure"));
        if (exposureAttr) exposureAttr.Get(&exposure);
        light.exposure = exposure;

        // Get world transform
        GfMatrix4d world_xform = xform_cache.GetLocalToWorldTransform(prim);
        matrix_to_float16(world_xform, light.transform);

        // ShapingAPI (spotlight cone, IES profiles)
        UsdLuxShapingAPI shaping(prim);
        if (shaping) {
            float coneAngle = 0.0f, coneSoftness = 0.0f, focus = 0.0f;
            shaping.GetShapingConeAngleAttr().Get(&coneAngle);
            shaping.GetShapingConeSoftnessAttr().Get(&coneSoftness);
            shaping.GetShapingFocusAttr().Get(&focus);
            light.shaping_cone_angle = coneAngle;
            light.shaping_cone_softness = coneSoftness;
            light.shaping_focus = focus;

            SdfAssetPath iesPath;
            if (shaping.GetShapingIesFileAttr().Get(&iesPath)) {
                if (!iesPath.GetResolvedPath().empty()) {
                    light.shaping_ies_file = iesPath.GetResolvedPath();
                } else if (!iesPath.GetAssetPath().empty()) {
                    light.shaping_ies_file = iesPath.GetAssetPath();
                }
            }
        }

        bridge->lights.push_back(std::move(light));
    }

    std::cout << "[USD_BRIDGE]   Cached " << bridge->lights.size() << " lights" << std::endl;
    bridge->lights_cached = true;
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

// Forward declarations for pre-caching
static void cache_stage_data(UsdBridgeStage* bridge);
static void cache_prim_data(UsdBridgeStage* bridge);
static void cache_animation_data(UsdBridgeStage* bridge);
static void cache_vertex_animation_data(UsdBridgeStage* bridge);
static void cache_light_data(UsdBridgeStage* bridge);

UsdBridgeError usd_bridge_open_stage(const char* path, UsdBridgeStage** out_stage) {
    if (!path || !out_stage) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    try {
        using namespace std::chrono;
        auto total_start = high_resolution_clock::now();

        // Normalize path: convert backslashes to forward slashes for USD
        std::string normalized_path(path);
        std::replace(normalized_path.begin(), normalized_path.end(), '\\', '/');

        std::cout << "[USD_BRIDGE] Opening stage: " << normalized_path << std::endl;

        // Get the asset resolver and create a context for relative path resolution.
        // This is critical for files that reference other files with relative paths
        // like @./lucy_low.usda@. Without the resolver context, USD can't find
        // the referenced files because it doesn't know the base directory.
        auto resolver_start = high_resolution_clock::now();
        ArResolver& resolver = ArGetResolver();
        ArResolverContext context = resolver.CreateDefaultContextForAsset(normalized_path);
        ArResolverContextBinder binder(context);
        auto resolver_time = duration_cast<milliseconds>(high_resolution_clock::now() - resolver_start).count();
        std::cout << "[USD_BRIDGE]   Resolver context: " << resolver_time << "ms" << std::endl;

        auto stage_open_start = high_resolution_clock::now();
        UsdStageRefPtr stage = UsdStage::Open(normalized_path);
        auto stage_open_time = duration_cast<milliseconds>(high_resolution_clock::now() - stage_open_start).count();
        std::cout << "[USD_BRIDGE]   UsdStage::Open(): " << stage_open_time << "ms" << std::endl;

        if (!stage) {
            std::cerr << "[USD_BRIDGE] ERROR: Failed to open stage" << std::endl;
            return USD_BRIDGE_ERROR_FILE_NOT_FOUND;
        }

        auto* bridge = new UsdBridgeStage();
        bridge->stage = stage;

        // Pre-cache all data at load time for thread safety.
        // All getter functions will read from these caches without mutation.
        // Note: The resolver context binder is still active during caching,
        // so all reference resolution during traversal will work correctly.
        auto cache_start = high_resolution_clock::now();
        cache_stage_data(bridge);
        auto cache_stage_time = duration_cast<milliseconds>(high_resolution_clock::now() - cache_start).count();
        std::cout << "[USD_BRIDGE]   cache_stage_data(): " << cache_stage_time << "ms"
                  << " (" << bridge->meshes.size() << " meshes, "
                  << bridge->instancers.size() << " instancers)" << std::endl;

        cache_start = high_resolution_clock::now();
        cache_prim_data(bridge);
        auto cache_prim_time = duration_cast<milliseconds>(high_resolution_clock::now() - cache_start).count();
        std::cout << "[USD_BRIDGE]   cache_prim_data(): " << cache_prim_time << "ms"
                  << " (" << bridge->all_prims.size() << " prims)" << std::endl;

        cache_start = high_resolution_clock::now();
        cache_animation_data(bridge);
        auto cache_anim_time = duration_cast<milliseconds>(high_resolution_clock::now() - cache_start).count();
        std::cout << "[USD_BRIDGE]   cache_animation_data(): " << cache_anim_time << "ms" << std::endl;

        cache_start = high_resolution_clock::now();
        cache_vertex_animation_data(bridge);
        auto cache_vert_anim_time = duration_cast<milliseconds>(high_resolution_clock::now() - cache_start).count();
        std::cout << "[USD_BRIDGE]   cache_vertex_animation_data(): " << cache_vert_anim_time << "ms" << std::endl;

        cache_start = high_resolution_clock::now();
        cache_light_data(bridge);
        auto cache_light_time = duration_cast<milliseconds>(high_resolution_clock::now() - cache_start).count();
        std::cout << "[USD_BRIDGE]   cache_light_data(): " << cache_light_time << "ms"
                  << " (" << bridge->lights.size() << " lights)" << std::endl;

        auto total_time = duration_cast<milliseconds>(high_resolution_clock::now() - total_start).count();
        std::cout << "[USD_BRIDGE]   TOTAL: " << total_time << "ms" << std::endl;

        *out_stage = bridge;
        return USD_BRIDGE_SUCCESS;

    } catch (const std::exception& e) {
        std::cerr << "[USD_BRIDGE] Exception: " << e.what() << std::endl;
        return USD_BRIDGE_ERROR_UNKNOWN;
    } catch (...) {
        std::cerr << "[USD_BRIDGE] Unknown exception" << std::endl;
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

    // Data pre-cached at load time - just read
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

    // Data pre-cached at load time - just read
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

    // Data pre-cached at load time - just read

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
    out_data->face_material_ids = mesh.face_material_ids.empty() ? nullptr : mesh.face_material_ids.data();
    out_data->triangle_count = mesh.face_material_ids.size();

    // Copy transform
    float mat_data[16];
    matrix_to_float16(mesh.transform, mat_data);
    for (int i = 0; i < 16; ++i) {
        out_data->transform[i] = mat_data[i];
    }

    out_data->purpose = static_cast<UsdBridgePurpose>(mesh.purpose);
    out_data->is_instance_proxy = mesh.is_instance_proxy ? 1 : 0;
    out_data->visibility = mesh.visible ? 1 : 0;
    out_data->double_sided = mesh.double_sided ? 1 : 0;
    out_data->subdivision_scheme = mesh.subdivision_scheme.c_str();
    out_data->normals_interpolation = mesh.normals_interpolation;
    out_data->display_color = mesh.display_color.empty() ? nullptr : mesh.display_color.data();
    out_data->display_color_count = mesh.display_color.size() / 3;
    out_data->display_opacity = mesh.display_opacity;
    out_data->resets_xform_stack = mesh.resets_xform_stack ? 1 : 0;

    // Polygon topology for subdivision surfaces
    out_data->face_vertex_counts = mesh.face_vertex_counts_orig.empty() ? nullptr : mesh.face_vertex_counts_orig.data();
    out_data->face_count = mesh.face_vertex_counts_orig.size();
    out_data->face_vertex_indices = mesh.face_vertex_indices_orig.empty() ? nullptr : mesh.face_vertex_indices_orig.data();
    out_data->face_vertex_index_count = mesh.face_vertex_indices_orig.size();

    // Crease data
    out_data->crease_indices = mesh.crease_indices.empty() ? nullptr : mesh.crease_indices.data();
    out_data->crease_index_count = mesh.crease_indices.size();
    out_data->crease_lengths = mesh.crease_lengths.empty() ? nullptr : mesh.crease_lengths.data();
    out_data->crease_length_count = mesh.crease_lengths.size();
    out_data->crease_sharpnesses = mesh.crease_sharpnesses.empty() ? nullptr : mesh.crease_sharpnesses.data();
    out_data->crease_sharpness_count = mesh.crease_sharpnesses.size();

    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_native_instance_count(
    const UsdBridgeStage* stage,
    size_t* out_count
) {
    if (!stage || !out_count) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }
    *out_count = stage->native_instances.size();
    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_native_instance(
    const UsdBridgeStage* stage,
    size_t index,
    UsdNativeInstanceData* out_data
) {
    if (!stage || !out_data) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }
    if (index >= stage->native_instances.size()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    const CachedNativeInstance& inst = stage->native_instances[index];
    out_data->proto_mesh_idx = inst.prototype_mesh_index;
    out_data->material_override_idx = inst.material_override_index;

    float mat_data[16];
    matrix_to_float16(inst.world_transform, mat_data);
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

    // Data pre-cached at load time - just read
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
    out_data->velocities = instancer.velocities.empty() ? nullptr : instancer.velocities.data();
    out_data->velocity_count = instancer.velocities.size() / 3;
    out_data->angular_velocities = instancer.angular_velocities.empty() ? nullptr : instancer.angular_velocities.data();
    out_data->angular_velocity_count = instancer.angular_velocities.size() / 3;
    out_data->invisible_ids = instancer.invisible_ids.empty() ? nullptr : instancer.invisible_ids.data();
    out_data->invisible_id_count = instancer.invisible_ids.size();

    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_material_count(
    const UsdBridgeStage* stage,
    size_t* out_count
) {
    if (!stage || !out_count) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    // Data pre-cached at load time - just read
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

    // Data pre-cached at load time - just read
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
    out_data->is_materialx = mat.is_materialx ? 1 : 0;

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

    // Data pre-cached at load time - just read
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

    // Data pre-cached at load time
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

    // Data pre-cached at load time

    if (index >= stage->all_prims.size()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    const CachedPrimInfo& info = stage->all_prims[index];
    out_info->path = info.path.c_str();
    out_info->type_name = info.type_name.c_str();
    out_info->is_active = info.is_active ? 1 : 0;
    out_info->has_children = info.has_children ? 1 : 0;
    out_info->child_count = info.child_count;
    out_info->visibility = info.visible ? 1 : 0;

    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_root_prim_count(
    const UsdBridgeStage* stage,
    size_t* out_count
) {
    if (!stage || !out_count) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    // Data pre-cached at load time
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

    // Data pre-cached at load time

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

    // Data pre-cached at load time

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

    // Data pre-cached at load time

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

    // Data pre-cached at load time

    std::string path_str(path);
    for (const auto& info : stage->all_prims) {
        if (info.path == path_str) {
            out_info->path = info.path.c_str();
            out_info->type_name = info.type_name.c_str();
            out_info->is_active = info.is_active ? 1 : 0;
            out_info->has_children = info.has_children ? 1 : 0;
            out_info->child_count = info.child_count;
            out_info->visibility = info.visible ? 1 : 0;
            return USD_BRIDGE_SUCCESS;
        }
    }

    return USD_BRIDGE_ERROR_INVALID_PRIM;
}

// ============================================================================
// Animation Data Caching
// ============================================================================

/// Cache animation data for all meshes and instancers
static void cache_animation_data(UsdBridgeStage* bridge) {
    if (bridge->animation_cached) return;

    // Ensure stage data is cached first
    cache_stage_data(bridge);

    bridge->mesh_animations.clear();
    bridge->instancer_animations.clear();
    bridge->camera_animations.clear();

    // Cache mesh animations
    for (size_t mesh_idx = 0; mesh_idx < bridge->meshes.size(); ++mesh_idx) {
        const auto& mesh = bridge->meshes[mesh_idx];
        UsdPrim prim = bridge->stage->GetPrimAtPath(SdfPath(mesh.path));
        if (!prim) {
            bridge->mesh_animations.push_back(CachedMeshAnimation{});
            continue;
        }

        CachedMeshAnimation anim;

        // Collect time samples from the prim AND all its ancestors
        // (animation may be on parent Xform, not the Mesh itself)
        std::set<double> time_set;
        UsdPrim current = prim;
        while (current) {
            UsdGeomXformable xformable(current);
            if (xformable) {
                std::vector<double> prim_times;
                xformable.GetTimeSamples(&prim_times);
                for (double t : prim_times) {
                    time_set.insert(t);
                }
            }
            current = current.GetParent();
        }

        // Convert set to sorted vector
        std::vector<double> times(time_set.begin(), time_set.end());

        if (!times.empty()) {
            UsdGeomXformCache xform_cache;
            for (double t : times) {
                CachedXformSample sample;
                sample.time = t;

                xform_cache.SetTime(UsdTimeCode(t));
                GfMatrix4d world_xform = xform_cache.GetLocalToWorldTransform(prim);
                matrix_to_float16(world_xform, sample.transform);

                anim.xform_samples.push_back(sample);
            }
        }

        bridge->mesh_animations.push_back(std::move(anim));
    }

    // Cache instancer animations
    for (size_t inst_idx = 0; inst_idx < bridge->instancers.size(); ++inst_idx) {
        const auto& instancer_data = bridge->instancers[inst_idx];
        UsdPrim prim = bridge->stage->GetPrimAtPath(SdfPath(instancer_data.path));
        if (!prim) {
            bridge->instancer_animations.push_back(CachedInstancerAnimation{});
            continue;
        }

        UsdGeomPointInstancer instancer(prim);
        if (!instancer) {
            bridge->instancer_animations.push_back(CachedInstancerAnimation{});
            continue;
        }

        CachedInstancerAnimation anim;
        anim.instance_count = instancer_data.transforms.size() / 16;

        // Get time samples from positions attribute (most common animated attribute)
        std::vector<double> times;
        instancer.GetPositionsAttr().GetTimeSamples(&times);

        if (times.empty()) {
            // Try orientations
            instancer.GetOrientationsAttr().GetTimeSamples(&times);
        }
        if (times.empty()) {
            // Try scales
            instancer.GetScalesAttr().GetTimeSamples(&times);
        }

        if (!times.empty()) {
            anim.time_samples = times;
            anim.transforms.reserve(times.size() * anim.instance_count * 16);

            for (double t : times) {
                VtArray<GfMatrix4d> instance_transforms;
                if (instancer.ComputeInstanceTransformsAtTime(
                        &instance_transforms,
                        UsdTimeCode(t),
                        UsdTimeCode(t))) {

                    for (const auto& mat : instance_transforms) {
                        float mat_data[16];
                        matrix_to_float16(mat, mat_data);
                        for (int i = 0; i < 16; ++i) {
                            anim.transforms.push_back(mat_data[i]);
                        }
                    }
                }
            }
        }

        bridge->instancer_animations.push_back(std::move(anim));
    }

    // Cache camera animations (add ALL cameras, not just animated ones)
    for (const UsdPrim& prim : bridge->stage->Traverse()) {
        if (!prim.IsA<UsdGeomCamera>()) continue;

        CachedCameraAnimation cam_anim;
        cam_anim.path = prim.GetPath().GetString();

        UsdGeomXformable xformable(prim);
        if (xformable) {
            std::vector<double> times;
            xformable.GetTimeSamples(&times);

            if (!times.empty()) {
                UsdGeomXformCache xform_cache;
                for (double t : times) {
                    CachedXformSample sample;
                    sample.time = t;

                    xform_cache.SetTime(UsdTimeCode(t));
                    GfMatrix4d world_xform = xform_cache.GetLocalToWorldTransform(prim);
                    matrix_to_float16(world_xform, sample.transform);

                    cam_anim.xform_samples.push_back(sample);
                }
            }
        }

        bridge->camera_animations.push_back(std::move(cam_anim));
    }

    bridge->animation_cached = true;
}

// ============================================================================
// Timeline / Animation API Implementation
// ============================================================================

UsdBridgeError usd_bridge_get_timeline(
    const UsdBridgeStage* stage,
    UsdBridgeTimelineData* out_data
) {
    if (!stage || !out_data) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    // Check if the stage has authored time metadata
    bool has_authored = stage->stage->HasAuthoredTimeCodeRange();

    if (has_authored) {
        out_data->start_time_code = stage->stage->GetStartTimeCode();
        out_data->end_time_code = stage->stage->GetEndTimeCode();
        out_data->has_authored_time_range = 1;
    } else {
        // Default values when no time range is authored
        out_data->start_time_code = 0.0;
        out_data->end_time_code = 0.0;
        out_data->has_authored_time_range = 0;
    }

    // FPS - use stage metadata or default to 24
    out_data->frames_per_second = stage->stage->GetFramesPerSecond();

    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_stage_metadata(
    const UsdBridgeStage* stage,
    UsdBridgeStageMetadata* out_data
) {
    if (!stage || !out_data) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    // metersPerUnit (default 0.01 = cm, per USD spec)
    out_data->meters_per_unit = UsdGeomGetStageMetersPerUnit(stage->stage);

    // upAxis (Y or Z)
    TfToken upAxis = UsdGeomGetStageUpAxis(stage->stage);
    if (upAxis == UsdGeomTokens->z) {
        out_data->up_axis = USD_BRIDGE_UP_AXIS_Z;
    } else {
        out_data->up_axis = USD_BRIDGE_UP_AXIS_Y;
    }

    // timeCodesPerSecond (default 24.0 per USD spec)
    out_data->time_codes_per_second = stage->stage->GetTimeCodesPerSecond();

    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_mesh_animation(
    const UsdBridgeStage* stage,
    size_t mesh_index,
    UsdBridgeAnimatedMeshData* out_data
) {
    if (!stage || !out_data) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    // Data pre-cached at load time

    if (mesh_index >= stage->mesh_animations.size()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    const auto& anim = stage->mesh_animations[mesh_index];
    out_data->mesh_index = mesh_index;

    if (anim.xform_samples.empty()) {
        out_data->xform_samples = nullptr;
        out_data->xform_sample_count = 0;
    } else {
        // Cast is safe: CachedXformSample has same layout as UsdBridgeXformSample
        out_data->xform_samples = reinterpret_cast<const UsdBridgeXformSample*>(anim.xform_samples.data());
        out_data->xform_sample_count = anim.xform_samples.size();
    }

    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_instancer_animation(
    const UsdBridgeStage* stage,
    size_t instancer_index,
    UsdBridgeAnimatedInstancerData* out_data
) {
    if (!stage || !out_data) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    // Data pre-cached at load time

    if (instancer_index >= stage->instancer_animations.size()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    const auto& anim = stage->instancer_animations[instancer_index];
    out_data->instancer_index = instancer_index;
    out_data->instance_count = anim.instance_count;

    if (anim.time_samples.empty()) {
        out_data->time_samples = nullptr;
        out_data->time_sample_count = 0;
        out_data->transforms = nullptr;
    } else {
        out_data->time_samples = anim.time_samples.data();
        out_data->time_sample_count = anim.time_samples.size();
        out_data->transforms = anim.transforms.data();
    }

    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_camera_xform_samples(
    const UsdBridgeStage* stage,
    const char* camera_path,
    const UsdBridgeXformSample** out_samples,
    size_t* out_count
) {
    if (!stage || !camera_path || !out_samples || !out_count) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    // Data pre-cached at load time

    std::string path_str(camera_path);

    for (const auto& cam_anim : stage->camera_animations) {
        if (cam_anim.path == path_str) {
            if (cam_anim.xform_samples.empty()) {
                *out_samples = nullptr;
                *out_count = 0;
            } else {
                *out_samples = reinterpret_cast<const UsdBridgeXformSample*>(cam_anim.xform_samples.data());
                *out_count = cam_anim.xform_samples.size();
            }
            return USD_BRIDGE_SUCCESS;
        }
    }

    // Camera not found or has no animation
    *out_samples = nullptr;
    *out_count = 0;
    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_camera_count(
    const UsdBridgeStage* stage,
    size_t* out_count
) {
    if (!stage || !out_count) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    *out_count = stage->camera_animations.size();
    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_camera_path(
    const UsdBridgeStage* stage,
    size_t index,
    const char** out_path
) {
    if (!stage || !out_path) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    if (index >= stage->camera_animations.size()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    *out_path = stage->camera_animations[index].path.c_str();
    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_camera_xform_at_time(
    const UsdBridgeStage* stage,
    const char* camera_path,
    double time,
    float* out_transform
) {
    if (!stage || !camera_path || !out_transform) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    // Find camera prim
    SdfPath path(camera_path);
    UsdPrim prim = stage->stage->GetPrimAtPath(path);
    if (!prim || !prim.IsA<UsdGeomCamera>()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    // Get xformable interface
    UsdGeomXformable xformable(prim);
    if (!xformable) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    // Evaluate transform at time
    GfMatrix4d localToWorld = xformable.ComputeLocalToWorldTransform(UsdTimeCode(time));

    // Flat copy of row-major USD data, matching matrix_to_float16 used for
    // meshes, lights, and instances. Rust interprets via from_cols_array().
    matrix_to_float16(localToWorld, out_transform);

    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_camera_properties(
    const UsdBridgeStage* stage,
    const char* camera_path,
    double time,
    UsdBridgeCameraProperties* out_props
) {
    if (!stage || !camera_path || !out_props) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    SdfPath path(camera_path);
    UsdPrim prim = stage->stage->GetPrimAtPath(path);
    if (!prim || !prim.IsA<UsdGeomCamera>()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    UsdGeomCamera camera(prim);
    UsdTimeCode tc(time);

    float focalLength = 50.0f;
    float verticalAperture = 24.89f;
    float horizontalAperture = 36.0f;
    float clipNear = 0.1f;
    float clipFar = 10000.0f;

    {
        float val;
        if (camera.GetFocalLengthAttr().Get(&val, tc)) {
            focalLength = val;
        }
    }
    {
        float val;
        if (camera.GetVerticalApertureAttr().Get(&val, tc)) {
            verticalAperture = val;
        }
    }
    {
        float val;
        if (camera.GetHorizontalApertureAttr().Get(&val, tc)) {
            horizontalAperture = val;
        }
    }
    {
        GfVec2f range;
        if (camera.GetClippingRangeAttr().Get(&range, tc)) {
            clipNear = range[0];
            clipFar = range[1];
        }
    }

    // Defense in depth: clamp invalid values to sane defaults
    // (Rust side also guards, but C++ bridge should not emit garbage)
    if (focalLength <= 0.0f) focalLength = 50.0f;
    if (verticalAperture <= 0.0f) verticalAperture = 24.89f;
    if (horizontalAperture <= 0.0f) horizontalAperture = 36.0f;
    if (clipNear <= 0.0f) clipNear = 0.1f;
    if (clipFar <= clipNear) clipFar = clipNear + 10000.0f;

    out_props->focal_length = focalLength;
    out_props->vertical_aperture = verticalAperture;
    out_props->horizontal_aperture = horizontalAperture;
    out_props->clip_near = clipNear;
    out_props->clip_far = clipFar;

    return USD_BRIDGE_SUCCESS;
}

// ============================================================================
// Vertex Animation
// ============================================================================

/// Cache vertex animation info for all meshes
static void cache_vertex_animation_data(UsdBridgeStage* bridge) {
    if (bridge->vertex_animation_cached) return;

    // Ensure stage data is cached first
    cache_stage_data(bridge);

    bridge->vertex_animations.clear();
    bridge->vertex_animations.reserve(bridge->meshes.size());

    for (size_t mesh_idx = 0; mesh_idx < bridge->meshes.size(); ++mesh_idx) {
        const auto& mesh = bridge->meshes[mesh_idx];
        UsdPrim prim = bridge->stage->GetPrimAtPath(SdfPath(mesh.path));

        CachedVertexAnimation anim;

        if (prim && prim.IsA<UsdGeomMesh>()) {
            UsdGeomMesh geomMesh(prim);
            UsdAttribute pointsAttr = geomMesh.GetPointsAttr();

            std::vector<double> timeSamples;
            if (pointsAttr.GetTimeSamples(&timeSamples) && timeSamples.size() > 1) {
                anim.has_animated_vertices = true;
                anim.time_samples = std::move(timeSamples);
            }
        }

        bridge->vertex_animations.push_back(std::move(anim));
    }

    bridge->vertex_animation_cached = true;
}

UsdBridgeError usd_bridge_get_mesh_vertex_animation_info(
    const UsdBridgeStage* stage,
    size_t mesh_index,
    UsdBridgeVertexAnimationInfo* out_info
) {
    if (!stage || !out_info) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    // Data pre-cached at load time

    if (mesh_index >= stage->vertex_animations.size()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    const auto& anim = stage->vertex_animations[mesh_index];
    out_info->has_animated_vertices = anim.has_animated_vertices ? 1 : 0;
    out_info->time_sample_count = anim.time_samples.size();
    out_info->time_samples = anim.time_samples.empty() ? nullptr : anim.time_samples.data();

    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_mesh_vertices_at_time(
    const UsdBridgeStage* stage,
    size_t mesh_index,
    double time,
    const float** out_vertices,
    size_t* out_vertex_count
) {
    if (!stage || !out_vertices || !out_vertex_count) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    // Data pre-cached at load time

    if (mesh_index >= stage->meshes.size()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    const auto& mesh = stage->meshes[mesh_index];
    const auto& anim = stage->vertex_animations[mesh_index];

    // If not animated, return cached static vertices
    if (!anim.has_animated_vertices) {
        *out_vertices = mesh.vertices.data();
        *out_vertex_count = mesh.vertices.size() / 3;
        return USD_BRIDGE_SUCCESS;
    }

    // Get mesh prim and query vertices at the specified time
    UsdPrim prim = stage->stage->GetPrimAtPath(SdfPath(mesh.path));
    if (!prim || !prim.IsA<UsdGeomMesh>()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    UsdGeomMesh geomMesh(prim);
    VtArray<GfVec3f> points;
    geomMesh.GetPointsAttr().Get(&points, UsdTimeCode(time));

    // Use thread_local buffer to avoid race conditions.
    // Each thread gets its own buffer, making this function thread-safe.
    static thread_local std::vector<float> return_buffer;
    return_buffer.clear();

    // If mesh was UV-split, expand animated positions to match split vertex count
    if (mesh.has_uv_split && !mesh.vertex_index_map.empty()) {
        size_t split_count = mesh.vertex_index_map.size();
        return_buffer.resize(split_count * 3);

        for (size_t i = 0; i < split_count; i++) {
            uint32_t orig_idx = mesh.vertex_index_map[i];
            if (orig_idx < points.size()) {
                return_buffer[i * 3 + 0] = points[orig_idx][0];
                return_buffer[i * 3 + 1] = points[orig_idx][1];
                return_buffer[i * 3 + 2] = points[orig_idx][2];
            } else {
                return_buffer[i * 3 + 0] = 0.0f;
                return_buffer[i * 3 + 1] = 0.0f;
                return_buffer[i * 3 + 2] = 0.0f;
            }
        }

        *out_vertices = return_buffer.data();
        *out_vertex_count = split_count;
    } else {
        // No UV split - return raw USD points
        return_buffer.reserve(points.size() * 3);
        for (const auto& p : points) {
            return_buffer.push_back(p[0]);
            return_buffer.push_back(p[1]);
            return_buffer.push_back(p[2]);
        }

        *out_vertices = return_buffer.data();
        *out_vertex_count = points.size();
    }

    return USD_BRIDGE_SUCCESS;
}

// ============================================================================
// Light Data API Implementation
// ============================================================================

UsdBridgeError usd_bridge_get_light_count(
    const UsdBridgeStage* stage,
    size_t* out_count
) {
    if (!stage || !out_count) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    // Data pre-cached at load time - just read
    *out_count = stage->lights.size();
    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_light(
    const UsdBridgeStage* stage,
    size_t index,
    UsdBridgeLightData* out_data
) {
    if (!stage || !out_data) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    // Data pre-cached at load time - just read
    if (index >= stage->lights.size()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    const CachedLight& light = stage->lights[index];
    out_data->path = light.path.c_str();
    out_data->type = light.type;
    out_data->color[0] = light.color[0];
    out_data->color[1] = light.color[1];
    out_data->color[2] = light.color[2];
    out_data->intensity = light.intensity;
    out_data->exposure = light.exposure;

    // Copy transform
    for (int i = 0; i < 16; ++i) {
        out_data->transform[i] = light.transform[i];
    }

    out_data->angle = light.angle;
    out_data->radius = light.radius;
    out_data->width = light.width;
    out_data->height = light.height;
    out_data->texture_path = light.texture_path.empty() ? nullptr : light.texture_path.c_str();
    out_data->length = light.length;
    out_data->shaping_cone_angle = light.shaping_cone_angle;
    out_data->shaping_cone_softness = light.shaping_cone_softness;
    out_data->shaping_focus = light.shaping_focus;
    out_data->shaping_ies_file = light.shaping_ies_file.empty() ? nullptr : light.shaping_ies_file.c_str();

    return USD_BRIDGE_SUCCESS;
}

// ============================================================================
// Edit Layer Export
// ============================================================================

// ============================================================================
// UsdGeomPoints API
// ============================================================================

UsdBridgeError usd_bridge_get_points_count(
    const UsdBridgeStage* stage,
    size_t* out_count
) {
    if (!stage || !out_count) return USD_BRIDGE_ERROR_NULL_POINTER;
    const_cast<UsdBridgeStage*>(stage)->points_cached || (cache_points_data(const_cast<UsdBridgeStage*>(stage)), true);
    *out_count = stage->points_prims.size();
    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_points(
    const UsdBridgeStage* stage,
    size_t index,
    UsdBridgePointsData* out_data
) {
    if (!stage || !out_data) return USD_BRIDGE_ERROR_NULL_POINTER;
    const_cast<UsdBridgeStage*>(stage)->points_cached || (cache_points_data(const_cast<UsdBridgeStage*>(stage)), true);

    if (index >= stage->points_prims.size()) return USD_BRIDGE_ERROR_INVALID_PRIM;

    const CachedPoints& pts = stage->points_prims[index];
    out_data->path = pts.path.c_str();
    out_data->positions = pts.positions.empty() ? nullptr : pts.positions.data();
    out_data->point_count = pts.positions.size() / 3;
    out_data->widths = pts.widths.empty() ? nullptr : pts.widths.data();
    out_data->width_count = pts.widths.size();
    out_data->normals = pts.normals.empty() ? nullptr : pts.normals.data();
    out_data->normal_count = pts.normals.size() / 3;
    out_data->ids = pts.ids.empty() ? nullptr : pts.ids.data();
    out_data->id_count = pts.ids.size();
    for (int i = 0; i < 16; ++i) out_data->transform[i] = pts.transform[i];

    return USD_BRIDGE_SUCCESS;
}

// ============================================================================
// Primvar Query API
// ============================================================================

UsdBridgeError usd_bridge_get_mesh_primvar_count(
    const UsdBridgeStage* stage,
    size_t mesh_index,
    size_t* out_count
) {
    if (!stage || !out_count) return USD_BRIDGE_ERROR_NULL_POINTER;

    // Lazy-cache primvars
    UsdBridgeStage* mutable_stage = const_cast<UsdBridgeStage*>(stage);
    if (mutable_stage->mesh_primvars.empty() && !mutable_stage->meshes.empty()) {
        cache_mesh_primvars(mutable_stage);
    }

    if (mesh_index >= stage->mesh_primvars.size()) {
        *out_count = 0;
        return USD_BRIDGE_SUCCESS;
    }
    *out_count = stage->mesh_primvars[mesh_index].size();
    return USD_BRIDGE_SUCCESS;
}

UsdBridgeError usd_bridge_get_mesh_primvar(
    const UsdBridgeStage* stage,
    size_t mesh_index,
    size_t primvar_index,
    UsdBridgePrimvarData* out_data
) {
    if (!stage || !out_data) return USD_BRIDGE_ERROR_NULL_POINTER;
    if (mesh_index >= stage->mesh_primvars.size()) return USD_BRIDGE_ERROR_INVALID_PRIM;
    if (primvar_index >= stage->mesh_primvars[mesh_index].size()) return USD_BRIDGE_ERROR_INVALID_PRIM;

    const CachedPrimvar& pv = stage->mesh_primvars[mesh_index][primvar_index];
    out_data->name = pv.name.c_str();
    out_data->type = pv.type;
    out_data->interpolation = pv.interpolation;
    out_data->float_data = pv.float_data.empty() ? nullptr : pv.float_data.data();
    out_data->int_data = pv.int_data.empty() ? nullptr : pv.int_data.data();
    out_data->element_count = pv.element_count;

    return USD_BRIDGE_SUCCESS;
}

struct UsdBridgeEditLayer {
    UsdStageRefPtr stage;
    std::string output_path;
};

UsdBridgeError usd_bridge_create_edit_layer(
    const char* output_path,
    UsdBridgeEditLayer** out_layer
) {
    if (!output_path || !out_layer) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    try {
        auto stage = UsdStage::CreateNew(output_path);
        if (!stage) {
            return USD_BRIDGE_ERROR_UNKNOWN;
        }

        auto* layer = new UsdBridgeEditLayer();
        layer->stage = stage;
        layer->output_path = output_path;
        *out_layer = layer;
        return USD_BRIDGE_SUCCESS;
    } catch (...) {
        return USD_BRIDGE_ERROR_UNKNOWN;
    }
}

UsdBridgeError usd_bridge_write_xform_opinion(
    UsdBridgeEditLayer* layer,
    const char* prim_path,
    double time,
    const float* matrix_16
) {
    if (!layer || !prim_path || !matrix_16) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    try {
        SdfPath path(prim_path);

        // Try override first (for sublayer composition over existing typed prims).
        // If the prim isn't xformable (typeless override on empty stage),
        // fall back to defining an Xform prim.
        auto prim = layer->stage->OverridePrim(path);
        if (!prim) {
            return USD_BRIDGE_ERROR_UNKNOWN;
        }

        UsdGeomXformable xformable(prim);
        if (!xformable) {
            prim = layer->stage->DefinePrim(path, TfToken("Xform"));
            if (!prim) {
                return USD_BRIDGE_ERROR_UNKNOWN;
            }
            xformable = UsdGeomXformable(prim);
            if (!xformable) {
                return USD_BRIDGE_ERROR_UNKNOWN;
            }
        }

        // Build GfMatrix4d from column-major float[16]
        GfMatrix4d mat;
        for (int col = 0; col < 4; ++col) {
            for (int row = 0; row < 4; ++row) {
                mat[row][col] = static_cast<double>(matrix_16[col * 4 + row]);
            }
        }

        // Clear existing xform ops and set single transform op
        bool reset_stack = false;
        auto ops = xformable.GetOrderedXformOps(&reset_stack);
        if (ops.empty()) {
            auto op = xformable.AddTransformOp();
            if (time < 0.0) {
                op.Set(mat, UsdTimeCode::Default());
            } else {
                op.Set(mat, UsdTimeCode(time));
            }
        } else {
            // Reuse existing transform op
            if (time < 0.0) {
                ops[0].Set(mat, UsdTimeCode::Default());
            } else {
                ops[0].Set(mat, UsdTimeCode(time));
            }
        }

        return USD_BRIDGE_SUCCESS;
    } catch (...) {
        return USD_BRIDGE_ERROR_UNKNOWN;
    }
}

UsdBridgeError usd_bridge_save_edit_layer(
    UsdBridgeEditLayer* layer
) {
    if (!layer) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    try {
        layer->stage->GetRootLayer()->Save();
        return USD_BRIDGE_SUCCESS;
    } catch (...) {
        return USD_BRIDGE_ERROR_UNKNOWN;
    }
}

void usd_bridge_free_edit_layer(
    UsdBridgeEditLayer* layer
) {
    delete layer;
}

UsdBridgeError usd_bridge_edit_layer_add_sublayer(
    UsdBridgeEditLayer* layer,
    const char* sublayer_path
) {
    if (!layer || !sublayer_path) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    try {
        auto rootLayer = layer->stage->GetRootLayer();
        rootLayer->InsertSubLayerPath(sublayer_path);
        return USD_BRIDGE_SUCCESS;
    } catch (const std::exception& e) {
        TF_WARN("usd_bridge_edit_layer_add_sublayer: %s", e.what());
        return USD_BRIDGE_ERROR_UNKNOWN;
    } catch (...) {
        TF_WARN("usd_bridge_edit_layer_add_sublayer: unknown exception");
        return USD_BRIDGE_ERROR_UNKNOWN;
    }
}

UsdBridgeError usd_bridge_edit_layer_add_reference(
    UsdBridgeEditLayer* layer,
    const char* prim_path,
    const char* reference_file,
    const char* reference_prim_path
) {
    if (!layer || !prim_path || !reference_file) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    try {
        SdfPath path(prim_path);
        auto prim = layer->stage->OverridePrim(path);
        if (!prim) {
            return USD_BRIDGE_ERROR_UNKNOWN;
        }

        SdfPath refPrimPath;
        if (reference_prim_path && reference_prim_path[0] != '\0') {
            refPrimPath = SdfPath(reference_prim_path);
        }

        auto refs = prim.GetReferences();
        refs.AddReference(SdfReference(reference_file, refPrimPath));
        return USD_BRIDGE_SUCCESS;
    } catch (const std::exception& e) {
        TF_WARN("usd_bridge_edit_layer_add_reference: %s", e.what());
        return USD_BRIDGE_ERROR_UNKNOWN;
    } catch (...) {
        TF_WARN("usd_bridge_edit_layer_add_reference: unknown exception");
        return USD_BRIDGE_ERROR_UNKNOWN;
    }
}

UsdBridgeError usd_bridge_edit_layer_set_default_prim(
    UsdBridgeEditLayer* layer,
    const char* prim_path
) {
    if (!layer || !prim_path) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    try {
        SdfPath path(prim_path);
        auto prim = layer->stage->GetPrimAtPath(path);
        if (!prim) {
            // Create the prim if it doesn't exist
            prim = layer->stage->OverridePrim(path);
        }
        if (!prim) {
            return USD_BRIDGE_ERROR_UNKNOWN;
        }
        layer->stage->SetDefaultPrim(prim);
        return USD_BRIDGE_SUCCESS;
    } catch (const std::exception& e) {
        TF_WARN("usd_bridge_edit_layer_set_default_prim: %s", e.what());
        return USD_BRIDGE_ERROR_UNKNOWN;
    } catch (...) {
        TF_WARN("usd_bridge_edit_layer_set_default_prim: unknown exception");
        return USD_BRIDGE_ERROR_UNKNOWN;
    }
}

UsdBridgeError usd_bridge_write_point_instancer(
    UsdBridgeEditLayer* layer,
    const char* prim_path,
    const float* positions,
    const float* orientations,
    const float* scales,
    const int32_t* proto_indices,
    size_t count,
    const char* const* prototype_paths,
    size_t prototype_count
) {
    if (!layer || !prim_path || !positions || !proto_indices ||
        !prototype_paths || count == 0 || prototype_count == 0) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    try {
        SdfPath path(prim_path);
        auto prim = layer->stage->DefinePrim(path, TfToken("PointInstancer"));
        if (!prim) {
            return USD_BRIDGE_ERROR_UNKNOWN;
        }

        UsdGeomPointInstancer instancer(prim);
        if (!instancer) {
            return USD_BRIDGE_ERROR_UNKNOWN;
        }

        // Set positions
        VtVec3fArray posArray(count);
        for (size_t i = 0; i < count; ++i) {
            posArray[i] = GfVec3f(
                positions[i * 3 + 0],
                positions[i * 3 + 1],
                positions[i * 3 + 2]
            );
        }
        instancer.GetPositionsAttr().Set(posArray);

        // Set orientations (quaternion wxyz -> GfQuath)
        VtQuathArray orientArray(count);
        for (size_t i = 0; i < count; ++i) {
            if (orientations) {
                orientArray[i] = GfQuath(
                    GfHalf(orientations[i * 4 + 0]),  // w
                    GfHalf(orientations[i * 4 + 1]),  // x
                    GfHalf(orientations[i * 4 + 2]),  // y
                    GfHalf(orientations[i * 4 + 3])   // z
                );
            } else {
                orientArray[i] = GfQuath::GetIdentity();
            }
        }
        instancer.GetOrientationsAttr().Set(orientArray);

        // Set scales
        VtVec3fArray scaleArray(count);
        for (size_t i = 0; i < count; ++i) {
            if (scales) {
                scaleArray[i] = GfVec3f(
                    scales[i * 3 + 0],
                    scales[i * 3 + 1],
                    scales[i * 3 + 2]
                );
            } else {
                scaleArray[i] = GfVec3f(1.0f, 1.0f, 1.0f);
            }
        }
        instancer.GetScalesAttr().Set(scaleArray);

        // Set prototype indices
        VtIntArray idxArray(count);
        for (size_t i = 0; i < count; ++i) {
            idxArray[i] = proto_indices[i];
        }
        instancer.GetProtoIndicesAttr().Set(idxArray);

        // Set prototype relationships
        auto protosRel = instancer.GetPrototypesRel();
        for (size_t i = 0; i < prototype_count; ++i) {
            if (prototype_paths[i]) {
                protosRel.AddTarget(SdfPath(prototype_paths[i]));
            }
        }

        return USD_BRIDGE_SUCCESS;
    } catch (const std::exception& e) {
        TF_WARN("usd_bridge_write_point_instancer: %s", e.what());
        return USD_BRIDGE_ERROR_UNKNOWN;
    } catch (...) {
        TF_WARN("usd_bridge_write_point_instancer: unknown exception");
        return USD_BRIDGE_ERROR_UNKNOWN;
    }
}

// ============================================================================
// Mesh Authoring
// ============================================================================

UsdBridgeError usd_bridge_write_mesh(
    UsdBridgeEditLayer* layer,
    const char* prim_path,
    const float* points,
    size_t point_count,
    const uint32_t* indices,
    size_t index_count,
    const float* normals,
    size_t normal_count,
    const float* uvs,
    size_t uv_count
) {
    if (!layer || !prim_path || !points || !indices ||
        point_count == 0 || index_count == 0 || (index_count % 3) != 0) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    try {
        SdfPath path(prim_path);
        auto prim = layer->stage->DefinePrim(path, TfToken("Mesh"));
        if (!prim) {
            return USD_BRIDGE_ERROR_UNKNOWN;
        }

        UsdGeomMesh mesh(prim);
        if (!mesh) {
            return USD_BRIDGE_ERROR_UNKNOWN;
        }

        // Polygonal (no subdivision)
        mesh.GetSubdivisionSchemeAttr().Set(TfToken("none"));

        // Points
        VtVec3fArray pointsArray(point_count);
        for (size_t i = 0; i < point_count; ++i) {
            pointsArray[i] = GfVec3f(
                points[i * 3 + 0],
                points[i * 3 + 1],
                points[i * 3 + 2]
            );
        }
        mesh.GetPointsAttr().Set(pointsArray);

        // Face vertex counts (all triangles = 3)
        size_t face_count = index_count / 3;
        VtIntArray faceVertexCounts(face_count, 3);
        mesh.GetFaceVertexCountsAttr().Set(faceVertexCounts);

        // Face vertex indices
        VtIntArray faceVertexIndices(index_count);
        for (size_t i = 0; i < index_count; ++i) {
            faceVertexIndices[i] = static_cast<int>(indices[i]);
        }
        mesh.GetFaceVertexIndicesAttr().Set(faceVertexIndices);

        // Normals (optional, vertex interpolation)
        if (normals && normal_count > 0) {
            VtVec3fArray normalsArray(normal_count);
            for (size_t i = 0; i < normal_count; ++i) {
                normalsArray[i] = GfVec3f(
                    normals[i * 3 + 0],
                    normals[i * 3 + 1],
                    normals[i * 3 + 2]
                );
            }
            mesh.GetNormalsAttr().Set(normalsArray);
            mesh.SetNormalsInterpolation(UsdGeomTokens->vertex);
        }

        // UVs (optional, vertex interpolation via primvars:st)
        if (uvs && uv_count > 0) {
            UsdGeomPrimvarsAPI primvarsAPI(mesh);
            auto stPrimvar = primvarsAPI.CreatePrimvar(
                TfToken("st"),
                SdfValueTypeNames->TexCoord2fArray,
                UsdGeomTokens->vertex
            );
            VtVec2fArray uvArray(uv_count);
            for (size_t i = 0; i < uv_count; ++i) {
                uvArray[i] = GfVec2f(uvs[i * 2 + 0], uvs[i * 2 + 1]);
            }
            stPrimvar.Set(uvArray);
        }

        // Extent (bounding box)
        GfVec3f minPt(FLT_MAX, FLT_MAX, FLT_MAX);
        GfVec3f maxPt(-FLT_MAX, -FLT_MAX, -FLT_MAX);
        for (size_t i = 0; i < point_count; ++i) {
            for (int c = 0; c < 3; ++c) {
                float v = points[i * 3 + c];
                if (v < minPt[c]) minPt[c] = v;
                if (v > maxPt[c]) maxPt[c] = v;
            }
        }
        VtVec3fArray extent = { minPt, maxPt };
        mesh.GetExtentAttr().Set(extent);

        return USD_BRIDGE_SUCCESS;
    } catch (const std::exception& e) {
        TF_WARN("usd_bridge_write_mesh: %s", e.what());
        return USD_BRIDGE_ERROR_UNKNOWN;
    } catch (...) {
        TF_WARN("usd_bridge_write_mesh: unknown exception");
        return USD_BRIDGE_ERROR_UNKNOWN;
    }
}

// ============================================================================
// Prim Authoring
// ============================================================================

UsdBridgeError usd_bridge_define_prim(
    UsdBridgeEditLayer* layer,
    const char* prim_path,
    const char* type_name,
    UsdBridgeSpecifier specifier
) {
    if (!layer || !prim_path) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    try {
        SdfPath path(prim_path);
        UsdPrim prim;

        if (specifier == USD_BRIDGE_SPECIFIER_OVER) {
            prim = layer->stage->OverridePrim(path);
        } else {
            TfToken typeTok(type_name ? type_name : "");
            prim = layer->stage->DefinePrim(path, typeTok);
        }

        if (!prim) {
            return USD_BRIDGE_ERROR_UNKNOWN;
        }

        return USD_BRIDGE_SUCCESS;
    } catch (const std::exception& e) {
        TF_WARN("usd_bridge_define_prim: %s", e.what());
        return USD_BRIDGE_ERROR_UNKNOWN;
    } catch (...) {
        TF_WARN("usd_bridge_define_prim: unknown exception");
        return USD_BRIDGE_ERROR_UNKNOWN;
    }
}

UsdBridgeError usd_bridge_set_prim_kind(
    UsdBridgeEditLayer* layer,
    const char* prim_path,
    UsdBridgeKind kind
) {
    if (!layer || !prim_path) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    try {
        SdfPath path(prim_path);
        auto prim = layer->stage->GetPrimAtPath(path);
        if (!prim) {
            return USD_BRIDGE_ERROR_INVALID_PRIM;
        }

        UsdModelAPI modelApi(prim);

        TfToken kindToken;
        switch (kind) {
            case USD_BRIDGE_KIND_COMPONENT:
                kindToken = KindTokens->component;
                break;
            case USD_BRIDGE_KIND_GROUP:
                kindToken = KindTokens->group;
                break;
            case USD_BRIDGE_KIND_ASSEMBLY:
                kindToken = KindTokens->assembly;
                break;
            case USD_BRIDGE_KIND_SUBCOMPONENT:
                kindToken = KindTokens->subcomponent;
                break;
            case USD_BRIDGE_KIND_NONE:
            default:
                return USD_BRIDGE_SUCCESS;
        }

        if (!modelApi.SetKind(kindToken)) {
            TF_WARN("usd_bridge_set_prim_kind: SetKind failed for '%s'", prim_path);
            return USD_BRIDGE_ERROR_UNKNOWN;
        }
        return USD_BRIDGE_SUCCESS;
    } catch (const std::exception& e) {
        TF_WARN("usd_bridge_set_prim_kind: %s", e.what());
        return USD_BRIDGE_ERROR_UNKNOWN;
    } catch (...) {
        TF_WARN("usd_bridge_set_prim_kind: unknown exception");
        return USD_BRIDGE_ERROR_UNKNOWN;
    }
}
