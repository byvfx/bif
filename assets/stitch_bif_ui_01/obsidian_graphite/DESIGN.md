# Design System Specification: Quiet Confidence

## 1. Overview & Creative North Star: "The Technical Atelier"
This design system is built for the high-end technical artist—a user who demands the precision of a CAD tool with the aesthetic fluidity of a bespoke creative suite. Our Creative North Star is **"The Technical Atelier."** 

Unlike generic "dark mode" enterprise tools that rely on heavy lines and cramped grids, this system treats the UI as a digital workspace of light and shadow. We move beyond the "template" look by utilizing **Tonal Architecture**: the belief that hierarchy is established through surface depth and optical gaps rather than structural outlines. The interface should feel like a high-end physical console—intentional, silent, and incredibly powerful.

---

### 2. Colors & Surface Architecture
The palette is rooted in deep neutrals to ensure the USD Layer colors and 3D viewport content remain the focal point.

#### The "No-Line" Rule
**Explicit Instruction:** Traditional 1px solid borders are prohibited for sectioning. Structural definition must be achieved through:
1.  **Background Shifts:** Placing a `surface_container_high` (#2a2a2a) panel against a `surface` (#131313) base.
2.  **Shadow Gaps:** Using the 1px `surface_container_lowest` (#0e0e0e) or a custom #111 shadow to create "optical "gutters" between panels.

#### Surface Hierarchy & Nesting
Treat the UI as a series of stacked, milled plates.
*   **Base Layer:** `surface` (#131313) — The foundation.
*   **Panel Layer:** `surface_container` (#20201f) — Primary work areas.
*   **Elevated/Menu Layer:** `surface_container_highest` (#353535) — Floating inspectors and context menus.
*   **Nesting:** When placing an input inside a panel, do not use a border. Use `surface_container_low` (#1b1b1b) to "cut" the input into the panel surface.

#### The "Glass & Gradient" Rule
For floating HUDs (Heads-Up Displays) within the 3D viewport, use **Glassmorphism**. Apply `surface_variant` (#353535) at 70% opacity with a `20px` backdrop-blur. Main action buttons (e.g., "Render" or "Export") should utilize a subtle linear gradient from `primary` (#a4c9ff) to `primary_container` (#4a9eff) to provide a "lit" appearance that feels premium and tactile.

---

### 3. Typography: Editorial Precision
The typography is designed to balance the density of USD data with the elegance of an editorial layout.

*   **The Power Scale:** We use **Manrope** for high-level navigation to provide a modern, wide-aperture feel, and **Inter** for UI labels to ensure maximum legibility at small sizes.
*   **Display & Headlines:** Use `headline-sm` (Manrope, 1.5rem) for major mode switching (Layout, Anim, Light).
*   **Section Headers:** Use `label-sm` (Inter, 0.6875rem), **Uppercase**, with `0.05em` letter-spacing. This creates an authoritative, "instrument-panel" aesthetic.
*   **Data Entry:** All property values and USD code strings must use `13px Monospace` (JetBrains Mono). This ensures that numerical columns align perfectly for easy scanning.

---

### 4. Elevation & Depth
In this system, depth is a function of light, not lines.

*   **The Layering Principle:** Avoid `z-index` chaos by sticking to the surface-container tiers. A card should never have a shadow if it is merely a sub-section; use a tonal shift to `surface_container_low`. 
*   **Ambient Shadows:** For floating popovers, use a "Soft Ambient" shadow: `0 8px 32px rgba(0, 0, 0, 0.4)`. The shadow color is never pure black, but a deeply desaturated version of the background to mimic real-world light occlusion.
*   **The "Ghost Border" Fallback:** If high-contrast accessibility is required, use the `outline_variant` (#414752) at **15% opacity**. This provides a "suggestion" of an edge without breaking the "Quiet Confidence" aesthetic.

---

### 5. Components

#### Buttons
*   **Primary:** `primary` (#a4c9ff) background, `on_primary` (#00315d) text. Radius: `md` (0.375rem). Use for the final "Commit" action.
*   **Secondary (Ghost):** No background. `outline` (#8a919e) text. On hover, transition to `surface_container_high` (#2a2a2a).
*   **Tertiary (Icon):** Minimalist. Use `on_surface_variant` (#c0c7d4). No container until hover.

#### Input Fields
*   **Default State:** `surface_container_lowest` (#0e0e0e) background. No border.
*   **Active/Focus State:** A 1px "Glow" using `primary_container` (#4a9eff). 
*   **Monospace Integration:** All numerical inputs must use `JetBrains Mono` for consistent character widths during value scrubbing.

#### The USD Layer List (Signature Component)
*   **The Stripe Rule:** Instead of full-row backgrounds for layer colors, use a 3px vertical "accent stripe" on the far left of the list item using the **USD Layer Colors** (Teal, Purple, etc.).
*   **Hierarchy:** Use the **Spacing Scale `2` (0.4rem)** to indent sub-layers. Do not use tree-lines; use the indentation and the vertical accent stripes to guide the eye.

#### Cards & Lists
*   **Rule:** Forbid divider lines. 
*   **Execution:** Use `Spacing Scale 3` (0.6rem) of vertical white space to separate logical groups. To separate individual list items, use a subtle background toggle between `surface_container` and `surface_container_low`.

---

### 6. Do’s and Don’ts

#### Do
*   **Do** use letter-spacing on all uppercase labels to improve "glanceability."
*   **Do** use "Optical Gaps" (1px of `surface_container_lowest`) to separate the Outliner, Viewport, and Inspector.
*   **Do** lean on the `secondary` (#5dd9d0) color for "Success" states—it feels more "Technical" than a standard green.

#### Don’t
*   **Don’t** use pure white (#ffffff) for text. It causes "haloing" in dark interfaces. Always use `on_surface` (#e5e2e1).
*   **Don’t** use sharp 90-degree corners. The `md` (0.375rem) radius is mandatory to maintain the "Atelier" feel.
*   **Don’t** use high-opacity shadows. If the shadow is visible at first glance, it is too heavy. It should be felt, not seen.