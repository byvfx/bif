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
#include <pxr/base/gf/matrix4f.h>
#include <pxr/base/gf/vec2f.h>
#include <pxr/base/gf/vec3f.h>
#include <pxr/base/gf/quath.h>
#include <pxr/base/vt/array.h>

#include <vector>
#include <string>
#include <memory>
#include <iostream>
#include <set>

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
    // Temporary buffer for vertices at a specific time (reused to avoid allocations)
    mutable std::vector<float> temp_vertices;
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
    bool animation_cached;

    // Animation caches
    std::vector<CachedMeshAnimation> mesh_animations;
    std::vector<CachedInstancerAnimation> instancer_animations;
    std::vector<CachedCameraAnimation> camera_animations;
    std::vector<CachedVertexAnimation> vertex_animations;
    bool vertex_animation_cached;

    UsdBridgeStage() : cached(false), prims_cached(false), materials_cached(false), animation_cached(false), vertex_animation_cached(false) {}

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

/// Convert GfMatrix4d to column-major float array
static void matrix_to_float16(const GfMatrix4d& mat, float* out) {
    GfMatrix4f matf(mat);
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

    // Cache materials first - needed for GeomSubset material assignment
    cache_material_data(bridge);

    UsdGeomXformCache xform_cache;

    // Traverse all prims
    for (const UsdPrim& prim : bridge->stage->Traverse()) {
        // Check for UsdGeomMesh
        if (prim.IsA<UsdGeomMesh>()) {
            UsdGeomMesh mesh(prim);
            CachedMesh cached;
            cached.path = prim.GetPath().GetString();

            // Get points at first time sample for animated geometry
            // Use stage's startTimeCode if available, otherwise first authored sample
            VtArray<GfVec3f> points;
            UsdTimeCode timeCode = UsdTimeCode::EarliestTime();

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
                std::cout << "[USD_BRIDGE] Mesh " << prim.GetPath() << " has animated points ("
                          << pointTimeSamples.size() << " samples), using time=" << timeCode.GetValue() << std::endl;
            }

            mesh.GetPointsAttr().Get(&points, timeCode);
            std::cout << "[USD_BRIDGE] Mesh " << prim.GetPath() << ": " << points.size() << " vertices" << std::endl;
            
            // Pre-allocate to exact size to minimize memory overhead
            cached.vertices.reserve(points.size() * 3);
            cached.vertices.shrink_to_fit();
            for (const auto& p : points) {
                cached.vertices.push_back(p[0]);
                cached.vertices.push_back(p[1]);
                cached.vertices.push_back(p[2]);
            }

            // Get face topology and triangulate (use same timeCode as points)
            VtArray<int> face_vertex_counts;
            VtArray<int> face_vertex_indices;
            mesh.GetFaceVertexCountsAttr().Get(&face_vertex_counts, timeCode);
            mesh.GetFaceVertexIndicesAttr().Get(&face_vertex_indices, timeCode);

            std::cout << "[USD_BRIDGE] Mesh " << prim.GetPath() << ": " << face_vertex_counts.size()
                      << " faces, " << face_vertex_indices.size() << " face vertex indices" << std::endl;

            std::vector<uint32_t> triangle_face_indices;
            triangulate_mesh(face_vertex_counts, face_vertex_indices, cached.indices, triangle_face_indices);

            // Extract GeomSubsets for per-face material assignment
            size_t num_faces = face_vertex_counts.size();
            std::vector<uint32_t> face_material_map(num_faces, 0);  // Default material 0

            std::vector<UsdGeomSubset> subsets = UsdGeomSubset::GetAllGeomSubsets(mesh);
            fprintf(stderr, "[USD_BRIDGE] Mesh %s: found %zu GeomSubsets, %zu faces\n",
                cached.path.c_str(), subsets.size(), num_faces);

            if (!subsets.empty()) {
                // Build material path -> index map
                std::map<std::string, uint32_t> material_path_to_index;
                for (size_t i = 0; i < bridge->materials.size(); ++i) {
                    material_path_to_index[bridge->materials[i].path] = static_cast<uint32_t>(i);
                    fprintf(stderr, "[USD_BRIDGE]   Material[%zu]: %s\n", i, bridge->materials[i].path.c_str());
                }

                for (const auto& subset : subsets) {
                    // Get material binding for this subset
                    UsdShadeMaterialBindingAPI binding_api(subset.GetPrim());
                    UsdShadeMaterial bound_material = binding_api.ComputeBoundMaterial();

                    // Skip subsets without valid material bindings (e.g., __subdivs__ from Houdini)
                    if (!bound_material) {
                        fprintf(stderr, "[USD_BRIDGE]   Skipping subset %s (no material binding)\n",
                            subset.GetPath().GetName().c_str());
                        continue;
                    }

                    std::string mat_path = bound_material.GetPath().GetString();
                    auto it = material_path_to_index.find(mat_path);
                    if (it == material_path_to_index.end()) {
                        fprintf(stderr, "[USD_BRIDGE]   WARNING: subset material '%s' not found in map!\n", mat_path.c_str());
                        continue;
                    }
                    uint32_t material_idx = it->second;

                    // Get face indices for this subset
                    VtArray<int> subset_indices;
                    subset.GetIndicesAttr().Get(&subset_indices);

                    fprintf(stderr, "[USD_BRIDGE]   Subset %s: %zu faces, material='%s' (idx=%u)\n",
                        subset.GetPath().GetName().c_str(), subset_indices.size(), mat_path.c_str(), material_idx);

                    // Assign material to these faces
                    for (int face_idx : subset_indices) {
                        if (face_idx >= 0 && static_cast<size_t>(face_idx) < num_faces) {
                            face_material_map[face_idx] = material_idx;
                        }
                    }
                }
            }

            // Map per-original-face materials to per-triangle
            cached.face_material_ids.reserve(triangle_face_indices.size());
            for (uint32_t orig_face : triangle_face_indices) {
                cached.face_material_ids.push_back(face_material_map[orig_face]);
            }

            // Get normals (optional) - track interpolation for UV seam split
            VtArray<GfVec3f> normals;
            TfToken normalsInterpolation;
            if (mesh.GetNormalsAttr().Get(&normals, timeCode)) {
                normalsInterpolation = mesh.GetNormalsInterpolation();
                fprintf(stderr, "[USD_BRIDGE] Normals: count=%zu, interpolation=%s\n",
                    normals.size(), normalsInterpolation.GetText());
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
                TfToken interpolation = stPrimvar.GetInterpolation();
                VtArray<GfVec2f> uvs;
                VtIntArray uvIndices;
                bool hasIndices = stPrimvar.GetIndices(&uvIndices, timeCode);

                if (stPrimvar.Get(&uvs, timeCode)) {
                    fprintf(stderr, "[USD_BRIDGE] UV primvar: interpolation=%s, uvs=%zu, indices=%zu, vertices=%zu, face_vertex_indices=%zu\n",
                        interpolation.GetText(), uvs.size(), uvIndices.size(), points.size(), face_vertex_indices.size());

                    if (interpolation == UsdGeomTokens->faceVarying) {
                        // faceVarying: one UV per face-vertex. Split vertices at UV seams.
                        // Map (original_vertex, uv) -> new_vertex_index
                        std::map<std::pair<int, std::pair<int,int>>, uint32_t> vertUvToNew;
                        std::vector<float> newVertices;
                        std::vector<float> newNormals;
                        std::vector<float> newUvs;
                        std::vector<uint32_t> newIndices;

                        newVertices.reserve(cached.vertices.size());
                        newNormals.reserve(cached.normals.size());
                        newUvs.reserve(face_vertex_indices.size() * 2);
                        newIndices.reserve(cached.indices.size());

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

                        size_t oldVertCount = cached.vertices.size() / 3;
                        size_t newVertCount = newVertices.size() / 3;
                        fprintf(stderr, "[USD_BRIDGE] UV seam split: %zu -> %zu vertices (%.1f%% increase)\n",
                            oldVertCount, newVertCount, 100.0 * (newVertCount - oldVertCount) / oldVertCount);

                        // Replace cached data with UV-split version
                        cached.vertices = std::move(newVertices);
                        cached.normals = std::move(newNormals);
                        cached.uvs = std::move(newUvs);
                        cached.indices = std::move(newIndices);

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
                    fprintf(stderr, "[USD_BRIDGE] Found standard_surface child: %s\n", child_name.c_str());
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

    // Also collect mesh-to-material bindings
    // Note: Material bindings can be inherited from parent prims
    bridge->mesh_material_paths.clear();
    for (const auto& mesh : bridge->meshes) {
        UsdPrim mesh_prim = bridge->stage->GetPrimAtPath(SdfPath(mesh.path));
        std::string mat_path;

        if (mesh_prim) {
            // Walk up the hierarchy to find material binding
            UsdPrim current = mesh_prim;
            while (current) {
                UsdShadeMaterialBindingAPI binding_api(current);
                UsdShadeMaterial bound_material = binding_api.ComputeBoundMaterial();
                if (bound_material) {
                    mat_path = bound_material.GetPath().GetString();
                    break;
                }
                current = current.GetParent();
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
    out_data->face_material_ids = mesh.face_material_ids.empty() ? nullptr : mesh.face_material_ids.data();
    out_data->triangle_count = mesh.face_material_ids.size();

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
            std::cout << "[USD_BRIDGE] Mesh " << mesh.path << " has " << times.size() << " time samples" << std::endl;
            UsdGeomXformCache xform_cache;
            for (double t : times) {
                CachedXformSample sample;
                sample.time = t;

                xform_cache.SetTime(UsdTimeCode(t));
                GfMatrix4d world_xform = xform_cache.GetLocalToWorldTransform(prim);
                matrix_to_float16(world_xform, sample.transform);

                // Debug: print translation component
                GfVec3d translation = world_xform.ExtractTranslation();
                std::cout << "  t=" << t << ": pos=(" << translation[0] << ", " << translation[1] << ", " << translation[2] << ")" << std::endl;

                anim.xform_samples.push_back(sample);
            }
        } else {
            std::cout << "[USD_BRIDGE] Mesh " << mesh.path << " has no animation" << std::endl;
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

    // Cache camera animations
    for (const UsdPrim& prim : bridge->stage->Traverse()) {
        if (!prim.IsA<UsdGeomCamera>()) continue;

        UsdGeomXformable xformable(prim);
        if (!xformable) continue;

        std::vector<double> times;
        xformable.GetTimeSamples(&times);

        if (!times.empty()) {
            CachedCameraAnimation cam_anim;
            cam_anim.path = prim.GetPath().GetString();

            UsdGeomXformCache xform_cache;
            for (double t : times) {
                CachedXformSample sample;
                sample.time = t;

                xform_cache.SetTime(UsdTimeCode(t));
                GfMatrix4d world_xform = xform_cache.GetLocalToWorldTransform(prim);
                matrix_to_float16(world_xform, sample.transform);

                cam_anim.xform_samples.push_back(sample);
            }

            bridge->camera_animations.push_back(std::move(cam_anim));
        }
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

UsdBridgeError usd_bridge_get_mesh_animation(
    const UsdBridgeStage* stage,
    size_t mesh_index,
    UsdBridgeAnimatedMeshData* out_data
) {
    if (!stage || !out_data) {
        return USD_BRIDGE_ERROR_NULL_POINTER;
    }

    cache_animation_data(const_cast<UsdBridgeStage*>(stage));

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

    cache_animation_data(const_cast<UsdBridgeStage*>(stage));

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

    cache_animation_data(const_cast<UsdBridgeStage*>(stage));

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
                std::cout << "[USD_BRIDGE] Mesh " << mesh.path << " has vertex animation ("
                          << anim.time_samples.size() << " time samples)" << std::endl;
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

    cache_vertex_animation_data(const_cast<UsdBridgeStage*>(stage));

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

    cache_vertex_animation_data(const_cast<UsdBridgeStage*>(stage));

    if (mesh_index >= stage->meshes.size()) {
        return USD_BRIDGE_ERROR_INVALID_PRIM;
    }

    const auto& mesh = stage->meshes[mesh_index];
    auto& anim = const_cast<CachedVertexAnimation&>(stage->vertex_animations[mesh_index]);

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

    // Copy to temp buffer
    anim.temp_vertices.clear();
    anim.temp_vertices.reserve(points.size() * 3);
    for (const auto& p : points) {
        anim.temp_vertices.push_back(p[0]);
        anim.temp_vertices.push_back(p[1]);
        anim.temp_vertices.push_back(p[2]);
    }

    *out_vertices = anim.temp_vertices.data();
    *out_vertex_count = points.size();

    return USD_BRIDGE_SUCCESS;
}
