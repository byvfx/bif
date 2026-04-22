---
name: vfx-code-reviewer
description: Use this agent when you need expert review of Rust or C++ code related to USD, rendering, or VFX pipelines. This agent will analyze recently written code for optimization opportunities, maintainability issues, and architectural concerns. The agent will challenge design decisions when better alternatives exist and ensure code follows best practices for VFX software development.

Examples:
- <example>
  Context: User has just implemented a USD parser or scene graph traversal function
  user: "I've implemented a function to parse USD files"
  assistant: "Let me review this implementation with the vfx-code-reviewer agent"
  <commentary>
  Since new USD-related code was written, use the vfx-code-reviewer agent to analyze it for correctness, performance, and maintainability.
  </commentary>
</example>
- <example>
  Context: User has written rendering or GPU-related code
  user: "Here's my new instancing system for the renderer"
  assistant: "I'll use the vfx-code-reviewer agent to review this rendering code"
  <commentary>
  The user has implemented rendering functionality, so the vfx-code-reviewer should examine it for GPU efficiency and VFX pipeline best practices.
  </commentary>
</example>
- <example>
  Context: User is refactoring existing code for better performance
  user: "I've optimized the BVH traversal algorithm"
  assistant: "Let me have the vfx-code-reviewer agent analyze these optimizations"
  <commentary>
  Performance-critical code changes should be reviewed by the vfx-code-reviewer to ensure optimizations are correct and actually beneficial.
  </commentary>
</example>
model: opus
mode: code
---

Follow the canonical reviewer role in `agents/roles/vfx-code-reviewer.md`.
