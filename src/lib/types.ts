export type TypedArray =
  | Uint8Array
  | Int8Array
  | Int16Array
  | Uint16Array
  | Int32Array
  | Uint32Array
  | Float32Array
  | Float64Array;

export type Orientation = {
  /** perm[rasAxis] = which voxel dim (0=i,1=j,2=k) maps to that RAS axis */
  perm: [number, number, number];
  /** flip[rasAxis] = whether the voxel dim runs opposite to the RAS direction */
  flip: [boolean, boolean, boolean];
  /** size along R, A, S respectively */
  rasSize: [number, number, number];
  /** voxel spacing (mm) in RAS order */
  voxdim: [number, number, number];
  /** RAS world coordinate (mm) of reoriented voxel (0,0,0) */
  rasOrigin: [number, number, number];
};

export type NiftiData = {
  header: import("nifti-reader-js").NIFTI1 | import("nifti-reader-js").NIFTI2;
  typedData: TypedArray;
  orientation: Orientation;
  min: number;
  max: number;
};

export type ViewPlane = "axial" | "coronal" | "sagittal";
