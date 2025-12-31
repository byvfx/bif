//! USDA (ASCII) file parser.
//!
//! This module provides line-by-line parsing of USDA text files.
//! The parser is intentionally simple and handles the most common USD patterns.
//!
//! # Supported Syntax
//!
//! - `def Xform "Name" { ... }`
//! - `def Mesh "Name" { ... }`
//! - `def PointInstancer "Name" { ... }`
//! - `float3[] points = [...]`
//! - `int[] faceVertexCounts = [...]`
//! - `int[] faceVertexIndices = [...]`
//! - `normal3f[] normals = [...]`
//! - `float3[] positions = [...]` (for PointInstancer)
//! - `quath[] orientations = [...]`
//! - `float3[] scales = [...]`
//! - `int[] protoIndices = [...]`
//! - `rel prototypes = [...]`
//! - `xformOp:translate`, `xformOp:rotateXYZ`, `xformOp:scale`

// TODO: Consider nom/pest for robustness if grammar complexity grows

use std::collections::VecDeque;

use bif_math::{Mat4, Quat, Vec3};
use thiserror::Error;

use super::types::*;

/// Errors that can occur during USDA parsing.
#[derive(Error, Debug)]
pub enum ParseError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Parse error at line {line}: {message}")]
    Parse { line: usize, message: String },
    
    #[error("Unexpected end of file")]
    UnexpectedEof,
    
    #[error("Invalid number format: {0}")]
    InvalidNumber(String),
    
    #[error("Unclosed block starting at line {0}")]
    UnclosedBlock(usize),
}

/// Result type for parsing operations.
pub type ParseResult<T> = Result<T, ParseError>;

/// USDA file parser.
pub struct UsdaParser {
    lines: VecDeque<(usize, String)>,
    current_line: usize,
}

impl UsdaParser {
    /// Create a new parser from file contents.
    pub fn new(content: &str) -> Self {
        let lines: VecDeque<_> = content
            .lines()
            .enumerate()
            .map(|(i, s)| (i + 1, s.to_string()))
            .collect();
        
        Self {
            lines,
            current_line: 0,
        }
    }
    
    /// Parse the USDA content and return a list of root prims.
    pub fn parse(&mut self) -> ParseResult<Vec<UsdPrim>> {
        let mut prims = Vec::new();
        
        // Skip header (lines starting with # and file-level metadata in parentheses)
        let mut in_header_metadata = false;
        while let Some((_, line)) = self.lines.front() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                self.lines.pop_front();
            } else if trimmed.starts_with('(') && !in_header_metadata {
                // Start of header metadata block
                in_header_metadata = true;
                // Check if it's a single-line metadata block
                if trimmed.ends_with(')') {
                    in_header_metadata = false;
                }
                self.lines.pop_front();
            } else if in_header_metadata {
                // Inside header metadata, consume until we find closing paren
                if trimmed.ends_with(')') || trimmed == ")" {
                    in_header_metadata = false;
                }
                self.lines.pop_front();
            } else {
                break;
            }
        }
        
        // Parse root prims
        while !self.lines.is_empty() {
            if let Some(prim) = self.parse_prim("")? {
                prims.push(prim);
            }
        }
        
        Ok(prims)
    }
    
    /// Parse a single prim and its children.
    fn parse_prim(&mut self, parent_path: &str) -> ParseResult<Option<UsdPrim>> {
        // Get next non-empty line
        let (line_num, line) = loop {
            match self.lines.pop_front() {
                Some((num, line)) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with('#') {
                        self.current_line = num;
                        break (num, line);
                    }
                }
                None => return Ok(None),
            }
        };
        
        let trimmed = line.trim();
        
        // Check for closing brace
        if trimmed == "}" {
            // Put it back for the caller to handle
            self.lines.push_front((line_num, line));
            return Ok(None);
        }
        
        // Parse prim definition: def Type "Name" { or def Type "Name" (
        if trimmed.starts_with("def ") {
            return self.parse_def(trimmed, parent_path, line_num);
        }
        
        // Skip other lines (attributes will be parsed within prim blocks)
        Ok(None)
    }
    
    /// Parse a `def Type "Name"` block.
    fn parse_def(&mut self, line: &str, parent_path: &str, start_line: usize) -> ParseResult<Option<UsdPrim>> {
        // Extract type and name: def Type "Name" {
        let rest = line.strip_prefix("def ").unwrap_or(line);
        
        // Find the type (first word)
        let mut parts = rest.split_whitespace();
        let prim_type = parts.next().unwrap_or("");
        
        // Find the name (quoted string)
        let name = if let Some(quote_start) = rest.find('"') {
            let after_quote = &rest[quote_start + 1..];
            if let Some(quote_end) = after_quote.find('"') {
                &after_quote[..quote_end]
            } else {
                ""
            }
        } else {
            ""
        };
        
        let path = if parent_path.is_empty() {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent_path, name)
        };
        
        // Check for metadata on this line or following lines
        // Metadata can be: inline (refs = @path@), multi-line starting on def line, or on next line
        let reference_info = if let Some(paren_start) = line.find('(') {
            if let Some(paren_end) = line.find(')') {
                // Inline metadata: def Type "Name" (metadata) { ... }
                let metadata = &line[paren_start..=paren_end];
                if metadata.contains("references") && metadata.contains('@') {
                    Some(self.parse_reference_line(metadata)?)
                } else {
                    None
                }
            } else {
                // Multi-line metadata started on def line: def Type "Name" (\n  metadata\n)
                // We need to consume lines until we find the closing paren
                self.parse_multiline_metadata_from_def()?
            }
        } else {
            // Check for metadata on next line (standalone parentheses)
            self.parse_metadata()?
        };
        
        // Check if brace and content are inline: def Type "Name" { ... }
        let has_inline_brace = line.contains('{');
        let has_inline_close = line.contains('}');
        
        // If entire prim is on one line: def Type "Name" (refs) { content }
        if has_inline_brace && has_inline_close {
            // Extract content between braces
            if let Some(brace_start) = line.find('{') {
                if let Some(brace_end) = line.rfind('}') {
                    let inline_content = line[brace_start + 1..brace_end].trim();
                    
                    // If we have a reference, create a reference prim with inline overrides
                    if let Some((asset_path, target_prim)) = reference_info {
                        return self.parse_inline_reference_content(&path, name, asset_path, target_prim, inline_content, start_line)
                            .map(|r| Some(UsdPrim::Reference(r)));
                    }
                    
                    // For non-reference single-line prims, create an empty transform
                    // (transform is parsed from inline_content if present)
                    return self.parse_inline_xform_content(&path, name, inline_content, start_line)
                        .map(|x| Some(UsdPrim::Xform(x)));
                }
            }
        }
        
        // Not inline - expect opening brace
        if !has_inline_brace {
            self.expect_opening_brace(start_line)?;
        }
        
        // If we found a reference, parse as a reference prim
        if let Some((asset_path, target_prim)) = reference_info {
            return self.parse_reference_content(&path, name, asset_path, target_prim, start_line)
                .map(|r| Some(UsdPrim::Reference(r)));
        }
        
        // Parse prim content based on type
        match prim_type {
            "Xform" => self.parse_xform_content(&path, name, start_line).map(|x| Some(UsdPrim::Xform(x))),
            "Mesh" => self.parse_mesh_content(&path, name, start_line).map(|m| Some(UsdPrim::Mesh(m))),
            "PointInstancer" => self.parse_point_instancer_content(&path, name, start_line).map(|p| Some(UsdPrim::PointInstancer(p))),
            "Scope" => {
                // Scope is like Xform but without transform
                self.parse_xform_content(&path, name, start_line).map(|x| Some(UsdPrim::Xform(x)))
            }
            _ => {
                // Skip unknown prim types
                self.skip_block(start_line)?;
                Ok(Some(UsdPrim::Unknown(prim_type.to_string())))
            }
        }
    }
    
    /// Parse metadata in parentheses and extract reference info if present.
    /// Returns Some((asset_path, target_prim_path)) if a reference was found.
    fn parse_metadata(&mut self) -> ParseResult<Option<(String, Option<String>)>> {
        // Look ahead for opening paren
        if let Some((_, line)) = self.lines.front() {
            let trimmed = line.trim();
            if trimmed.starts_with('(') || trimmed.ends_with('(') {
                // Collect all metadata lines
                let mut metadata_lines = Vec::new();
                let mut depth = 1;
                self.lines.pop_front();
                
                while depth > 0 {
                    match self.lines.pop_front() {
                        Some((_, line)) => {
                            depth += line.matches('(').count();
                            depth -= line.matches(')').count();
                            if depth > 0 || !line.trim().starts_with(')') {
                                metadata_lines.push(line);
                            }
                        }
                        None => return Err(ParseError::UnexpectedEof),
                    }
                }
                
                // Look for references = @path@</prim> pattern
                for line in &metadata_lines {
                    if line.contains("references") && line.contains('@') {
                        return Ok(Some(self.parse_reference_line(line)?));
                    }
                }
            }
        }
        Ok(None)
    }
    
    /// Parse multi-line metadata when the opening paren is on the def line.
    /// e.g., def Xform "Name" (\n    kind = "component"\n)
    fn parse_multiline_metadata_from_def(&mut self) -> ParseResult<Option<(String, Option<String>)>> {
        // The opening paren was on the def line, so we start at depth 1
        let mut metadata_lines = Vec::new();
        let mut depth = 1;
        
        while depth > 0 {
            match self.lines.pop_front() {
                Some((_, line)) => {
                    depth += line.matches('(').count();
                    depth -= line.matches(')').count();
                    if depth > 0 || !line.trim().starts_with(')') {
                        metadata_lines.push(line);
                    }
                }
                None => return Err(ParseError::UnexpectedEof),
            }
        }
        
        // Look for references = @path@</prim> pattern
        for line in &metadata_lines {
            if line.contains("references") && line.contains('@') {
                return Ok(Some(self.parse_reference_line(line)?));
            }
        }
        
        Ok(None)
    }
    
    /// Parse a reference line like: references = @./lucy.usda@</Lucy>
    fn parse_reference_line(&self, line: &str) -> ParseResult<(String, Option<String>)> {
        // Find the asset path between @ symbols
        let first_at = line.find('@');
        let second_at = line.rfind('@');
        
        if let (Some(start), Some(end)) = (first_at, second_at) {
            if start < end {
                let asset_path = line[start + 1..end].to_string();
                
                // Look for target prim path after the closing @
                let after_ref = &line[end + 1..];
                let target_prim = if let Some(prim_start) = after_ref.find('<') {
                    if let Some(prim_end) = after_ref.find('>') {
                        Some(after_ref[prim_start + 1..prim_end].to_string())
                    } else {
                        None
                    }
                } else {
                    None
                };
                
                return Ok((asset_path, target_prim));
            }
        }
        
        Err(ParseError::Parse {
            line: self.current_line,
            message: format!("Invalid reference syntax: {}", line),
        })
    }
    
    /// Parse content of a prim that has a reference (may have overrides).
    fn parse_reference_content(
        &mut self,
        path: &str,
        name: &str,
        asset_path: String,
        target_prim: Option<String>,
        start_line: usize,
    ) -> ParseResult<UsdReference> {
        let mut reference = UsdReference {
            path: path.to_string(),
            name: name.to_string(),
            asset_path,
            target_prim_path: target_prim,
            transform: Mat4::IDENTITY,
            children: Vec::new(),
        };
        
        let mut xform_ops = Vec::new();
        
        loop {
            let (line_num, line) = match self.lines.pop_front() {
                Some(x) => x,
                None => return Err(ParseError::UnclosedBlock(start_line)),
            };
            
            let trimmed = line.trim();
            
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            
            if trimmed == "}" {
                break;
            }
            
            // Parse xformOps (transform overrides)
            if let Some(op) = self.parse_xform_op(trimmed)? {
                xform_ops.push(op);
                continue;
            }
            
            // Check for child prim (overrides)
            if trimmed.starts_with("def ") {
                self.lines.push_front((line_num, line));
                if let Some(child) = self.parse_prim(path)? {
                    reference.children.push(child);
                }
                continue;
            }
        }
        
        reference.transform = compose_xform_ops(&xform_ops);
        
        Ok(reference)
    }
    
    /// Parse content of a single-line prim that has a reference with inline content.
    /// e.g., def Xform "Lucy_0_0" (references = @./lucy_low.usda@) { double3 xformOp:translate = (0, 0, 0) }
    fn parse_inline_reference_content(
        &self,
        path: &str,
        name: &str,
        asset_path: String,
        target_prim: Option<String>,
        inline_content: &str,
        _start_line: usize,
    ) -> ParseResult<UsdReference> {
        let mut reference = UsdReference {
            path: path.to_string(),
            name: name.to_string(),
            asset_path,
            target_prim_path: target_prim,
            transform: Mat4::IDENTITY,
            children: Vec::new(),
        };
        
        let xform_ops = self.parse_inline_xform_ops(inline_content)?;
        reference.transform = compose_xform_ops(&xform_ops);
        
        Ok(reference)
    }
    
    /// Parse content of a single-line Xform with inline content.
    fn parse_inline_xform_content(
        &self,
        path: &str,
        name: &str,
        inline_content: &str,
        _start_line: usize,
    ) -> ParseResult<UsdXform> {
        let mut xform = UsdXform {
            path: path.to_string(),
            name: name.to_string(),
            transform: Mat4::IDENTITY,
            children: Vec::new(),
        };
        
        let xform_ops = self.parse_inline_xform_ops(inline_content)?;
        xform.transform = compose_xform_ops(&xform_ops);
        
        Ok(xform)
    }
    
    /// Parse xformOps from inline content (semicolon or space-separated attributes).
    fn parse_inline_xform_ops(&self, content: &str) -> ParseResult<Vec<XformOp>> {
        let mut ops = Vec::new();
        
        // Split by common delimiters (could be space or semicolon separated)
        // For now, just look for xformOp patterns
        for part in content.split(';') {
            let part = part.trim();
            if let Some(op) = self.parse_xform_op(part)? {
                ops.push(op);
            }
        }
        
        // Also try parsing the whole thing as a single xformOp
        if ops.is_empty() {
            if let Some(op) = self.parse_xform_op(content.trim())? {
                ops.push(op);
            }
        }
        
        Ok(ops)
    }

    /// Expect and consume an opening brace.
    fn expect_opening_brace(&mut self, start_line: usize) -> ParseResult<()> {
        // The brace might be on the same line as def, or on the next line
        // We've already consumed the def line, so check if we need to find the brace
        while let Some((_num, line)) = self.lines.front() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                self.lines.pop_front();
                continue;
            }
            if trimmed == "{" {
                self.lines.pop_front();
                return Ok(());
            }
            // Brace might have been on the def line
            return Ok(());
        }
        
        Err(ParseError::Parse {
            line: start_line,
            message: "Expected opening brace".to_string(),
        })
    }
    
    /// Skip a block (consume until matching closing brace).
    fn skip_block(&mut self, start_line: usize) -> ParseResult<()> {
        let mut depth = 1;
        
        while depth > 0 {
            match self.lines.pop_front() {
                Some((_, line)) => {
                    depth += line.matches('{').count();
                    depth -= line.matches('}').count();
                }
                None => return Err(ParseError::UnclosedBlock(start_line)),
            }
        }
        
        Ok(())
    }
    
    /// Parse Xform content (transform ops and children).
    fn parse_xform_content(&mut self, path: &str, name: &str, start_line: usize) -> ParseResult<UsdXform> {
        let mut xform = UsdXform {
            path: path.to_string(),
            name: name.to_string(),
            transform: Mat4::IDENTITY,
            children: Vec::new(),
        };
        
        let mut xform_ops = Vec::new();
        
        loop {
            let (line_num, line) = match self.lines.pop_front() {
                Some(x) => x,
                None => return Err(ParseError::UnclosedBlock(start_line)),
            };
            
            let trimmed = line.trim();
            
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            
            if trimmed == "}" {
                break;
            }
            
            // Check for child prim FIRST (before xformOps, since def lines may contain xformOp text)
            if trimmed.starts_with("def ") {
                self.lines.push_front((line_num, line));
                if let Some(child) = self.parse_prim(path)? {
                    xform.children.push(child);
                }
                continue;
            }
            
            // Parse xformOps
            if let Some(op) = self.parse_xform_op(trimmed)? {
                xform_ops.push(op);
                continue;
            }
        }
        
        // Compose xformOps into final transform
        xform.transform = compose_xform_ops(&xform_ops);
        
        Ok(xform)
    }
    
    /// Parse Mesh content.
    fn parse_mesh_content(&mut self, path: &str, name: &str, start_line: usize) -> ParseResult<UsdMesh> {
        let mut mesh = UsdMesh {
            path: path.to_string(),
            name: name.to_string(),
            ..Default::default()
        };
        
        let mut xform_ops = Vec::new();
        
        loop {
            let (_, line) = match self.lines.pop_front() {
                Some(x) => x,
                None => return Err(ParseError::UnclosedBlock(start_line)),
            };
            
            let trimmed = line.trim();
            
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            
            if trimmed == "}" {
                break;
            }
            
            // Parse xformOps
            if let Some(op) = self.parse_xform_op(trimmed)? {
                xform_ops.push(op);
                continue;
            }
            
            // Parse points
            if trimmed.contains("point3f[]") && trimmed.contains("points") {
                mesh.points = self.parse_vec3_array(trimmed)?;
                continue;
            }
            
            if trimmed.contains("float3[]") && trimmed.contains("points") {
                mesh.points = self.parse_vec3_array(trimmed)?;
                continue;
            }
            
            // Parse face vertex counts
            if trimmed.contains("faceVertexCounts") {
                mesh.face_vertex_counts = self.parse_int_array(trimmed)?;
                continue;
            }
            
            // Parse face vertex indices
            if trimmed.contains("faceVertexIndices") {
                mesh.face_vertex_indices = self.parse_int_array(trimmed)?;
                continue;
            }
            
            // Parse normals
            if trimmed.contains("normal3f[]") && trimmed.contains("normals") {
                mesh.normals = Some(self.parse_vec3_array(trimmed)?);
                continue;
            }
            
            if trimmed.contains("float3[]") && trimmed.contains("normals") {
                mesh.normals = Some(self.parse_vec3_array(trimmed)?);
                continue;
            }
            
            // Parse orientation (winding order)
            if trimmed.contains("orientation") {
                if trimmed.contains("\"leftHanded\"") {
                    mesh.left_handed = true;
                    log::debug!("Mesh {} uses left-handed winding", mesh.name);
                }
                continue;
            }
        }
        
        mesh.transform = compose_xform_ops(&xform_ops);
        
        Ok(mesh)
    }
    
    /// Parse PointInstancer content.
    fn parse_point_instancer_content(&mut self, path: &str, name: &str, start_line: usize) -> ParseResult<UsdPointInstancer> {
        let mut instancer = UsdPointInstancer {
            path: path.to_string(),
            name: name.to_string(),
            ..Default::default()
        };
        
        let mut xform_ops = Vec::new();
        
        loop {
            let (line_num, line) = match self.lines.pop_front() {
                Some(x) => x,
                None => return Err(ParseError::UnclosedBlock(start_line)),
            };
            
            let trimmed = line.trim();
            
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            
            if trimmed == "}" {
                break;
            }
            
            // Parse xformOps
            if let Some(op) = self.parse_xform_op(trimmed)? {
                xform_ops.push(op);
                continue;
            }
            
            // Parse positions
            if (trimmed.contains("point3f[]") || trimmed.contains("float3[]")) && trimmed.contains("positions") {
                instancer.positions = self.parse_vec3_array(trimmed)?;
                continue;
            }
            
            // Parse protoIndices
            if trimmed.contains("protoIndices") {
                instancer.proto_indices = self.parse_int_array(trimmed)?;
                continue;
            }
            
            // Parse orientations (quaternions)
            if trimmed.contains("orientations") {
                instancer.orientations = Some(self.parse_quat_array(trimmed)?);
                continue;
            }
            
            // Parse scales
            if (trimmed.contains("float3[]") || trimmed.contains("point3f[]")) && trimmed.contains("scales") {
                instancer.scales = Some(self.parse_vec3_array(trimmed)?);
                continue;
            }
            
            // Parse prototypes relationship
            if trimmed.contains("rel prototypes") {
                instancer.prototypes = self.parse_rel_array(trimmed)?;
                continue;
            }
            
            // Check for child prim (inline prototypes)
            if trimmed.starts_with("def ") {
                self.lines.push_front((line_num, line));
                if let Some(child) = self.parse_prim(path)? {
                    instancer.children.push(child);
                }
                continue;
            }
        }
        
        instancer.transform = compose_xform_ops(&xform_ops);
        
        Ok(instancer)
    }
    
    /// Parse a single xformOp attribute.
    fn parse_xform_op(&self, line: &str) -> ParseResult<Option<XformOp>> {
        // Skip xformOpOrder - it just lists the order of ops, not actual values
        if line.contains("xformOpOrder") {
            return Ok(None);
        }
        
        // xformOp:translate = (1, 2, 3)
        if line.contains("xformOp:translate") && line.contains('=') && line.contains('(') {
            let vec = self.parse_inline_vec3(line)?;
            return Ok(Some(XformOp::Translate(vec)));
        }
        
        // xformOp:rotateX = 90
        if line.contains("xformOp:rotateX") && line.contains('=') && !line.contains("xformOp:rotateXYZ") {
            let val = self.parse_inline_float(line)?;
            return Ok(Some(XformOp::RotateX(val)));
        }
        
        // xformOp:rotateY = 90
        if line.contains("xformOp:rotateY") && line.contains('=') {
            let val = self.parse_inline_float(line)?;
            return Ok(Some(XformOp::RotateY(val)));
        }
        
        // xformOp:rotateZ = 90
        if line.contains("xformOp:rotateZ") && line.contains('=') {
            let val = self.parse_inline_float(line)?;
            return Ok(Some(XformOp::RotateZ(val)));
        }
        
        // xformOp:rotateXYZ = (0, 45, 0)
        if line.contains("xformOp:rotateXYZ") && line.contains('=') {
            let vec = self.parse_inline_vec3(line)?;
            return Ok(Some(XformOp::RotateXYZ(vec)));
        }
        
        // xformOp:scale = (1, 1, 1)
        if line.contains("xformOp:scale") && line.contains('=') {
            let vec = self.parse_inline_vec3(line)?;
            return Ok(Some(XformOp::Scale(vec)));
        }
        
        Ok(None)
    }
    
    /// Parse an inline Vec3 value like (1, 2, 3).
    /// For lines with xformOp, finds the value after the xformOp's = sign.
    fn parse_inline_vec3(&self, line: &str) -> ParseResult<Vec3> {
        // Find the xformOp pattern and look for = after it
        // This handles lines like: { double3 xformOp:translate = (0, 0, 0) }
        // where there might be other ( ) pairs earlier in the line
        let search_start = if let Some(xform_pos) = line.find("xformOp:") {
            // Find the = after the xformOp
            if let Some(eq_offset) = line[xform_pos..].find('=') {
                xform_pos + eq_offset
            } else {
                0
            }
        } else if let Some(eq_pos) = line.find('=') {
            eq_pos
        } else {
            0
        };
        
        let search_str = &line[search_start..];
        
        // Find parentheses in the portion after the relevant =
        let start = search_str.find('(').ok_or_else(|| ParseError::Parse {
            line: self.current_line,
            message: format!("Expected '(' in: {}", line),
        })?;
        
        let end = search_str.find(')').ok_or_else(|| ParseError::Parse {
            line: self.current_line,
            message: format!("Expected ')' in: {}", line),
        })?;
        
        let inner = &search_str[start + 1..end];
        let parts: Vec<&str> = inner.split(',').collect();
        
        if parts.len() != 3 {
            return Err(ParseError::Parse {
                line: self.current_line,
                message: format!("Expected 3 components, got {}", parts.len()),
            });
        }
        
        let x = parts[0].trim().parse::<f32>().map_err(|_| ParseError::InvalidNumber(parts[0].to_string()))?;
        let y = parts[1].trim().parse::<f32>().map_err(|_| ParseError::InvalidNumber(parts[1].to_string()))?;
        let z = parts[2].trim().parse::<f32>().map_err(|_| ParseError::InvalidNumber(parts[2].to_string()))?;
        
        Ok(Vec3::new(x, y, z))
    }
    
    /// Parse an inline float value.
    fn parse_inline_float(&self, line: &str) -> ParseResult<f32> {
        // Find = and parse what comes after
        let eq_pos = line.find('=').ok_or_else(|| ParseError::Parse {
            line: self.current_line,
            message: "Expected '='".to_string(),
        })?;
        
        let value_str = line[eq_pos + 1..].trim();
        value_str.parse::<f32>().map_err(|_| ParseError::InvalidNumber(value_str.to_string()))
    }
    
    /// Parse a Vec3 array like [(1, 2, 3), (4, 5, 6), ...].
    fn parse_vec3_array(&mut self, first_line: &str) -> ParseResult<Vec<Vec3>> {
        let mut result = Vec::new();
        let mut content = String::new();
        
        // Find the = sign first, then look for [ after it
        let eq_pos = first_line.find('=').unwrap_or(0);
        let after_eq = &first_line[eq_pos..];
        
        // Check if array is on this line or spans multiple lines
        if let Some(bracket_start) = after_eq.find('[') {
            content.push_str(&after_eq[bracket_start..]);
            
            // If closing bracket not found, read more lines
            if !content.contains(']') {
                while let Some((_, line)) = self.lines.pop_front() {
                    content.push_str(&line);
                    if line.contains(']') {
                        break;
                    }
                }
            }
        }
        
        // Extract between brackets
        let start = content.find('[').unwrap_or(0) + 1;
        let end = content.find(']').unwrap_or(content.len());
        let inner = &content[start..end];
        
        // Parse (x, y, z) tuples
        let mut chars = inner.chars().peekable();
        while let Some(&c) = chars.peek() {
            if c == '(' {
                chars.next(); // consume '('
                let mut tuple_str = String::new();
                
                while let Some(&tc) = chars.peek() {
                    if tc == ')' {
                        chars.next(); // consume ')'
                        break;
                    }
                    tuple_str.push(chars.next().unwrap());
                }
                
                let parts: Vec<&str> = tuple_str.split(',').collect();
                if parts.len() == 3 {
                    let x = parts[0].trim().parse::<f32>().unwrap_or(0.0);
                    let y = parts[1].trim().parse::<f32>().unwrap_or(0.0);
                    let z = parts[2].trim().parse::<f32>().unwrap_or(0.0);
                    result.push(Vec3::new(x, y, z));
                }
            } else {
                chars.next();
            }
        }
        
        Ok(result)
    }
    
    /// Parse an int array like [1, 2, 3, ...].
    fn parse_int_array(&mut self, first_line: &str) -> ParseResult<Vec<i32>> {
        let mut content = String::new();
        
        // Find the = sign first, then look for [ after it
        let eq_pos = first_line.find('=').unwrap_or(0);
        let after_eq = &first_line[eq_pos..];
        
        // Check if array is on this line or spans multiple lines
        if let Some(bracket_start) = after_eq.find('[') {
            content.push_str(&after_eq[bracket_start..]);
            
            // If closing bracket not found, read more lines
            if !content.contains(']') {
                while let Some((_, line)) = self.lines.pop_front() {
                    content.push_str(" ");
                    content.push_str(&line);
                    if line.contains(']') {
                        break;
                    }
                }
            }
        }
        
        // Extract between brackets
        let start = content.find('[').unwrap_or(0) + 1;
        let end = content.find(']').unwrap_or(content.len());
        let inner = &content[start..end];
        
        // Parse comma-separated integers
        let result: Vec<i32> = inner
            .split(',')
            .filter_map(|s| s.trim().parse::<i32>().ok())
            .collect();
        
        Ok(result)
    }
    
    /// Parse a quaternion array like [(1, 0, 0, 0), ...].
    fn parse_quat_array(&mut self, first_line: &str) -> ParseResult<Vec<Quat>> {
        let mut result = Vec::new();
        let mut content = String::new();
        
        // Find the = sign first, then look for [ after it
        let eq_pos = first_line.find('=').unwrap_or(0);
        let after_eq = &first_line[eq_pos..];
        
        if let Some(bracket_start) = after_eq.find('[') {
            content.push_str(&after_eq[bracket_start..]);
            
            if !content.contains(']') {
                while let Some((_, line)) = self.lines.pop_front() {
                    content.push_str(&line);
                    if line.contains(']') {
                        break;
                    }
                }
            }
        }
        
        let start = content.find('[').unwrap_or(0) + 1;
        let end = content.find(']').unwrap_or(content.len());
        let inner = &content[start..end];
        
        // Parse (w, x, y, z) or (x, y, z, w) tuples
        let mut chars = inner.chars().peekable();
        while let Some(&c) = chars.peek() {
            if c == '(' {
                chars.next();
                let mut tuple_str = String::new();
                
                while let Some(&tc) = chars.peek() {
                    if tc == ')' {
                        chars.next();
                        break;
                    }
                    tuple_str.push(chars.next().unwrap());
                }
                
                let parts: Vec<&str> = tuple_str.split(',').collect();
                if parts.len() == 4 {
                    // USD uses (imaginary x, imaginary y, imaginary z, real w) format for quath
                    let x = parts[0].trim().parse::<f32>().unwrap_or(0.0);
                    let y = parts[1].trim().parse::<f32>().unwrap_or(0.0);
                    let z = parts[2].trim().parse::<f32>().unwrap_or(0.0);
                    let w = parts[3].trim().parse::<f32>().unwrap_or(1.0);
                    result.push(Quat::from_xyzw(x, y, z, w));
                }
            } else {
                chars.next();
            }
        }
        
        Ok(result)
    }
    
    /// Parse a relationship array like [</Path/To/Prim>, ...].
    fn parse_rel_array(&mut self, first_line: &str) -> ParseResult<Vec<String>> {
        let mut result = Vec::new();
        let mut content = String::new();
        
        if let Some(bracket_start) = first_line.find('[') {
            content.push_str(&first_line[bracket_start..]);
            
            if !content.contains(']') {
                while let Some((_, line)) = self.lines.pop_front() {
                    content.push_str(&line);
                    if line.contains(']') {
                        break;
                    }
                }
            }
        }
        
        // Extract paths between < and >
        let mut chars = content.chars().peekable();
        while let Some(&c) = chars.peek() {
            if c == '<' {
                chars.next();
                let mut path = String::new();
                
                while let Some(&pc) = chars.peek() {
                    if pc == '>' {
                        chars.next();
                        break;
                    }
                    path.push(chars.next().unwrap());
                }
                
                if !path.is_empty() {
                    result.push(path);
                }
            } else {
                chars.next();
            }
        }
        
        Ok(result)
    }
}

/// Parse a USDA string and return the list of root prims.
pub fn parse_usda(content: &str) -> ParseResult<Vec<UsdPrim>> {
    let mut parser = UsdaParser::new(content);
    parser.parse()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple_mesh() {
        let usda = r#"
def Mesh "Cube" {
    point3f[] points = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)]
    int[] faceVertexCounts = [4]
    int[] faceVertexIndices = [0, 1, 2, 3]
}
"#;
        
        let prims = parse_usda(usda).unwrap();
        assert_eq!(prims.len(), 1);
        
        if let UsdPrim::Mesh(mesh) = &prims[0] {
            assert_eq!(mesh.name, "Cube");
            assert_eq!(mesh.points.len(), 4);
            assert_eq!(mesh.face_vertex_counts, vec![4]);
            assert_eq!(mesh.face_vertex_indices, vec![0, 1, 2, 3]);
        } else {
            panic!("Expected Mesh prim");
        }
    }
    
    #[test]
    fn test_parse_xform_with_ops() {
        let usda = r#"
def Xform "Model" {
    double3 xformOp:translate = (1, 2, 3)
    double3 xformOp:scale = (2, 2, 2)
    uniform token[] xformOpOrder = ["xformOp:translate", "xformOp:scale"]
}
"#;
        
        let prims = parse_usda(usda).unwrap();
        assert_eq!(prims.len(), 1);
        
        if let UsdPrim::Xform(xform) = &prims[0] {
            assert_eq!(xform.name, "Model");
            // Transform should have translation and scale applied
            let translated = xform.transform.transform_point3(Vec3::ZERO);
            assert!((translated - Vec3::new(1.0, 2.0, 3.0)).length() < 0.001);
        } else {
            panic!("Expected Xform prim");
        }
    }
    
    #[test]
    fn test_parse_point_instancer() {
        let usda = r#"
def PointInstancer "Instances" {
    int[] protoIndices = [0, 0, 0]
    point3f[] positions = [(0, 0, 0), (2, 0, 0), (4, 0, 0)]
    rel prototypes = [</Instances/Proto>]
    
    def Mesh "Proto" {
        point3f[] points = [(0, 0, 0), (1, 0, 0), (0.5, 1, 0)]
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
    }
}
"#;
        
        let prims = parse_usda(usda).unwrap();
        assert_eq!(prims.len(), 1);
        
        if let UsdPrim::PointInstancer(instancer) = &prims[0] {
            assert_eq!(instancer.name, "Instances");
            assert_eq!(instancer.positions.len(), 3);
            assert_eq!(instancer.proto_indices, vec![0, 0, 0]);
            assert_eq!(instancer.children.len(), 1);
        } else {
            panic!("Expected PointInstancer prim");
        }
    }
}
