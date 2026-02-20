import type * as nifti from "nifti-reader-js";
import type { Orientation } from "./types";

/**
 * Determine which voxel dimensions correspond to R, A, S
 * by examining the 3×3 rotation part of the affine.
 */
export function computeOrientation(
  header: nifti.NIFTI1 | nifti.NIFTI2,
): Orientation {
  const affine = header.affine; // affine[worldRow][voxelCol]
  const perm: [number, number, number] = [0, 0, 0];
  const flip: [boolean, boolean, boolean] = [false, false, false];
  const used = [false, false, false];

  for (let rasAxis = 0; rasAxis < 3; rasAxis++) {
    let maxVal = 0;
    let bestVox = 0;
    for (let voxAxis = 0; voxAxis < 3; voxAxis++) {
      if (used[voxAxis]) continue;
      const val = Math.abs(affine[rasAxis][voxAxis]);
      if (val > maxVal) {
        maxVal = val;
        bestVox = voxAxis;
      }
    }
    perm[rasAxis] = bestVox;
    flip[rasAxis] = affine[rasAxis][bestVox] < 0;
    used[bestVox] = true;
  }

  const rasSize: [number, number, number] = [
    header.dims[perm[0] + 1],
    header.dims[perm[1] + 1],
    header.dims[perm[2] + 1],
  ];

  // Voxel spacings reordered to RAS
  const voxdim: [number, number, number] = [
    Math.abs(header.pixDims[perm[0] + 1]),
    Math.abs(header.pixDims[perm[1] + 1]),
    Math.abs(header.pixDims[perm[2] + 1]),
  ];

  // Compute the RAS coordinate of reoriented voxel (0,0,0).
  // After permutation + flip, new voxel 0 along axis a came from
  // original axis perm[a] at index (shape-1 if flipped, 0 otherwise).
  const origIJK = [0, 0, 0];
  for (let a = 0; a < 3; a++) {
    const v = perm[a];
    origIJK[v] = flip[a] ? header.dims[v + 1] - 1 : 0;
  }
  const rasOrigin: [number, number, number] = [0, 0, 0];
  for (let row = 0; row < 3; row++) {
    rasOrigin[row] =
      affine[row][0] * origIJK[0] +
      affine[row][1] * origIJK[1] +
      affine[row][2] * origIJK[2] +
      affine[row][3];
  }

  return { perm, flip, rasSize, voxdim, rasOrigin };
}

/**
 * Convert a voxel index to display mm along the given RAS axis.
 * Axis 0 (R) is negated so the sagittal slider reads in L-positive
 * convention; axes 1 (A) and 2 (S) are kept as-is to match 3D Slicer.
 */
export function voxelToMm(
  orient: Orientation,
  axis: number,
  idx: number,
): number {
  const ras = orient.rasOrigin[axis] + idx * orient.voxdim[axis];
  return axis === 0 ? -ras : ras;
}

/** Convert a display mm value back to the nearest voxel index. */
export function mmToVoxel(
  orient: Orientation,
  axis: number,
  mm: number,
): number {
  const rasMm = axis === 0 ? -mm : mm;
  const n = orient.rasSize[axis];
  const idx = Math.round(
    (rasMm - orient.rasOrigin[axis]) / orient.voxdim[axis],
  );
  return Math.max(0, Math.min(n - 1, idx));
}
