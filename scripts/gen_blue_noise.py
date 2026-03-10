"""Generate a 256x256 blue noise texture using void-and-cluster algorithm.

Outputs a raw binary file (65,536 bytes) where each byte is a blue noise
value in [0, 255]. Used by bif_renderer::blue_noise for Cranley-Patterson
rotation sampling.

Usage:
    python scripts/gen_blue_noise.py
"""

import numpy as np
from pathlib import Path


def generate_blue_noise(size: int = 256) -> np.ndarray:
    """Generate blue noise via void-and-cluster (simplified).

    Uses iterative rank assignment with Gaussian energy to produce
    a well-distributed dither pattern.
    """
    rng = np.random.default_rng(42)
    n = size * size

    # Start with ~10% seed points
    initial_density = 0.1
    binary = rng.random((size, size)) < initial_density

    sigma = 1.5  # Gaussian filter sigma

    # Build Gaussian kernel (wrap-aware via FFT)
    ax = np.arange(size)
    ax = np.minimum(ax, size - ax).astype(np.float64)
    xx, yy = np.meshgrid(ax, ax)
    kernel = np.exp(-(xx**2 + yy**2) / (2 * sigma**2))
    kernel_fft = np.fft.fft2(kernel)

    def energy(pattern):
        return np.real(np.fft.ifft2(np.fft.fft2(pattern.astype(np.float64)) * kernel_fft))

    # Phase 1: Remove seed points to find tightest clusters
    rank = np.zeros((size, size), dtype=np.int32)
    current = binary.copy()
    count = int(current.sum())

    for r in range(count, 0, -1):
        e = energy(current)
        # Find tightest cluster (highest energy among set pixels)
        e[~current] = -np.inf
        idx = np.unravel_index(np.argmax(e), e.shape)
        rank[idx] = r - 1
        current[idx] = False

    # Phase 2: Add points to voids
    current = binary.copy()
    for r in range(count, n):
        e = energy(current)
        # Find largest void (lowest energy among unset pixels)
        e[current] = np.inf
        idx = np.unravel_index(np.argmin(e), e.shape)
        rank[idx] = r
        current[idx] = True

    # Normalize to [0, 255]
    rank_normalized = (rank.astype(np.float64) / (n - 1) * 255).astype(np.uint8)
    return rank_normalized


def main():
    size = 256
    print(f"Generating {size}x{size} blue noise texture...")
    texture = generate_blue_noise(size)

    out_path = Path(__file__).parent.parent / "crates" / "bif_renderer" / "src" / "data" / "blue_noise_256x256.bin"
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "wb") as f:
        f.write(texture.tobytes())

    print(f"Written {out_path} ({out_path.stat().st_size} bytes)")

    # Verify properties
    values = texture.flatten()
    print(f"  Min: {values.min()}, Max: {values.max()}, Mean: {values.mean():.1f}")
    unique = len(np.unique(values))
    print(f"  Unique values: {unique}/256")


if __name__ == "__main__":
    main()
