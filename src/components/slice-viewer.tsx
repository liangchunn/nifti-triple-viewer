import { useEffect, useRef, useState } from "react";
import { Slider } from "./ui/slider";
import type { NiftiData, ViewPlane } from "../lib/types";
import { voxelToMm, mmToVoxel } from "../lib/orientation";
import { render } from "../lib/renderer";
import { cn } from "@/lib/utils";
import { useMediaQuery } from "@/hooks/use-media-query";

interface NiftiSliceViewerProps {
  data: NiftiData;
  viewPlane: ViewPlane;
  contrast: number;
  brightness: number;
  className?: string | undefined;
}

const COLOR: Record<ViewPlane, string> = {
  axial: "text-yellow-400",
  coronal: "text-green-400",
  sagittal: "text-red-400",
};

export function NiftiSliceViewer({
  data,
  viewPlane,
  contrast,
  brightness,
  className,
}: NiftiSliceViewerProps) {
  const canvas = useRef<HTMLCanvasElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const sliceAxis = viewPlane === "axial" ? 2 : viewPlane === "coronal" ? 1 : 0;
  const [currentSlice, setCurrentSlice] = useState(() =>
    Math.floor(data.orientation.rasSize[sliceAxis] / 2),
  );
  const [focused, setFocused] = useState(false);

  const orient = data.orientation;
  const mmA = voxelToMm(orient, sliceAxis, 0);
  const mmB = voxelToMm(orient, sliceAxis, orient.rasSize[sliceAxis] - 1);
  const mmMin = Math.min(mmA, mmB);
  const mmMax = Math.max(mmA, mmB);
  const mmStep = orient.voxdim[sliceAxis];
  const mmValue = voxelToMm(orient, sliceAxis, currentSlice);
  const axisLabel = sliceAxis === 0 ? "X" : sliceAxis === 1 ? "Y" : "Z";

  const maxSlice = data.orientation.rasSize[sliceAxis] - 1;
  const isMobile = useMediaQuery("(max-width: 639px)");

  // Activate scroll on click, deactivate on click outside
  useEffect(() => {
    if (isMobile) return;
    const el = containerRef.current;
    if (!el) return;
    const onMouseDown = () => setFocused(true);
    const onClickOutside = (e: MouseEvent) => {
      if (!el.contains(e.target as Node)) setFocused(false);
    };
    el.addEventListener("mousedown", onMouseDown);
    document.addEventListener("mousedown", onClickOutside);
    return () => {
      el.removeEventListener("mousedown", onMouseDown);
      document.removeEventListener("mousedown", onClickOutside);
    };
  }, [isMobile]);

  // Scroll to change slice (only when focused)
  useEffect(() => {
    if (isMobile) return;
    if (!focused) return;
    const el = containerRef.current;
    if (!el) return;
    let accumulator = 0;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      accumulator += e.deltaY;
      const step = Math.trunc(accumulator / 30);
      if (step !== 0) {
        accumulator -= step * 30;
        setCurrentSlice((prev) => Math.max(0, Math.min(maxSlice, prev - step)));
      }
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [focused, maxSlice, isMobile]);

  useEffect(() => {
    render(data, currentSlice, canvas, viewPlane, contrast, brightness);
  }, [data, currentSlice, viewPlane, contrast, brightness]);

  return (
    <div
      className={cn(
        "w-full sm:min-h-0 sm:flex sm:flex-col relative",
        className,
      )}
    >
      <div
        ref={containerRef}
        className={cn(
          "w-full aspect-square sm:aspect-auto sm:flex-1 sm:min-h-0 overflow-hidden flex items-center justify-center bg-black border",
          focused ? "border-gray-600" : "border-transparent",
        )}
      >
        <canvas ref={canvas} className="block w-full h-full object-contain" />
      </div>
      <div className="absolute top-0 left-0 w-full">
        <div className="flex justify-between items-center px-2 py-1">
          <label
            className={cn("text-sm font-medium tabular-nums", COLOR[viewPlane])}
          >
            {viewPlane.charAt(0).toUpperCase() + viewPlane.slice(1)} {axisLabel}{" "}
            = {mmValue.toFixed(1)} mm
          </label>
          <label
            className={cn(
              "text-xs font-medium tabular-nums opacity-50 font-mono",
              COLOR[viewPlane],
            )}
          >
            {currentSlice}/{maxSlice}
          </label>
        </div>
      </div>
      <div className="absolute bottom-2 left-0 w-full opacity-50 px-2">
        <Slider
          onValueChange={(vals) => {
            setCurrentSlice(mmToVoxel(orient, sliceAxis, vals[0]));
          }}
          value={[mmValue]}
          min={mmMin}
          max={mmMax}
          step={mmStep}
          className="w-full"
        />
      </div>
      {/* <div className="w-full pb-2 px-2 shrink-0 flex">
        <Slider
          onValueChange={(vals) => {
            setCurrentSlice(mmToVoxel(orient, sliceAxis, vals[0]));
          }}
          value={[mmValue]}
          min={mmMin}
          max={mmMax}
          step={mmStep}
        />
      </div> */}
    </div>
  );
}
