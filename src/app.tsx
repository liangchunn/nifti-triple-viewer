import { useState } from "react";
import { NiftiSliceViewer } from "./components/slice-viewer";
import { Slider } from "./components/ui/slider";
import type { NiftiData } from "./lib/types";
import { loadNifti } from "./lib/nifti-utils";
import { useDropzone } from "react-dropzone";
import { Button } from "./components/ui/button";
import {
  ContrastIcon,
  FolderOpenIcon,
  ImportIcon,
  SunIcon,
} from "lucide-react";
import { toast } from "sonner";
import {
  Menubar,
  MenubarContent,
  MenubarGroup,
  MenubarItem,
  MenubarMenu,
  MenubarPortal,
  MenubarSeparator,
  MenubarTrigger,
} from "@/components/ui/menubar";

function App() {
  const [data, setData] = useState<null | NiftiData>(null);
  const [contrast, setContrast] = useState(1);
  const [brightness, setBrightness] = useState(0);
  const [fileName, setFileName] = useState<null | string>(null);

  const onDrop = async (acceptedFiles: File[]) => {
    try {
      const file = acceptedFiles[0];
      if (file) {
        const contents = await file.arrayBuffer();
        const data = await loadNifti(contents);
        setData(data);
        setFileName(file.name);
      }
    } catch (e) {
      toast.error(`Failed to load file: ${e}`);
    }
  };

  const { getRootProps, getInputProps, isDragActive, open } = useDropzone({
    onDrop,
    multiple: false,
  });

  const clearFile = () => {
    setData(null);
    setFileName(null);
  };

  return (
    <div
      className="sm:h-svh sm:overflow-hidden sm:flex sm:flex-col"
      {...getRootProps({
        onClick: (e) => e.stopPropagation(),
      })}
    >
      <input {...getInputProps()} />
      <div className="fixed sm:static w-full z-10 px-2 pt-2">
        <Menubar>
          <MenubarMenu>
            <MenubarTrigger>File</MenubarTrigger>
            <MenubarPortal>
              <MenubarContent>
                <MenubarGroup>
                  <MenubarItem onClick={open}>Load file...</MenubarItem>
                </MenubarGroup>
                <MenubarSeparator />
                <MenubarGroup>
                  <MenubarItem onClick={clearFile}>Unload file</MenubarItem>
                </MenubarGroup>
              </MenubarContent>
            </MenubarPortal>
          </MenubarMenu>
          <label className="text-muted-foreground text-sm ml-1 overflow-hidden whitespace-nowrap text-ellipsis">
            {fileName}
          </label>
          <div className="flex-1" />
          <div className="flex gap-4 mr-1">
            <div className="flex w-36 gap-2">
              <ContrastIcon className="size-4 opacity-50" />
              <Slider
                onValueChange={(vals) => setContrast(vals[0])}
                value={[contrast]}
                min={0.1}
                max={3}
                step={0.1}
                className="opacity-50"
                disabled={data === null}
              />
            </div>
            <div className="flex w-36 gap-2">
              <SunIcon className="size-4 opacity-50" />
              <Slider
                onValueChange={(vals) => setBrightness(vals[0])}
                value={[brightness]}
                min={-128}
                max={128}
                step={1}
                className="opacity-50"
                disabled={data === null}
              />
            </div>
          </div>
        </Menubar>
      </div>
      {!data && (
        <div className="grid h-svh items-center justify-center">
          <div className="flex flex-col items-center gap-2">
            <p className="text-2xl font-medium pb-2">No volume loaded</p>
            <Button onClick={open}>
              <FolderOpenIcon className="size-4" />
              Load NIfTI file
            </Button>
            <p className="text-muted-foreground">
              or drag and drop .nii/.nii.gz files here
            </p>
          </div>
        </div>
      )}
      {isDragActive && (
        <div className="bg-background/80 w-full h-full fixed top-0 left-0 z-10 flex items-center justify-center flex-col">
          <div>
            <ImportIcon className="animate-bounce size-8" />
          </div>
          <p className="text-xl font-medium">Drop to load file...</p>
        </div>
      )}
      {data && (
        <div
          className="pt-13 sm:pt-0 sm:flex-1 sm:min-h-0 sm:overflow-hidden"
          key={data.typedData.byteLength}
        >
          <div className="grid grid-cols-1 sm:grid-cols-2 sm:grid-rows-2 mx-auto sm:h-full">
            <NiftiSliceViewer
              data={data}
              viewPlane={"axial"}
              contrast={contrast}
              brightness={brightness}
            />
            <NiftiSliceViewer
              className="sm:row-span-2"
              data={data}
              viewPlane={"sagittal"}
              contrast={contrast}
              brightness={brightness}
            />
            <NiftiSliceViewer
              data={data}
              viewPlane={"coronal"}
              contrast={contrast}
              brightness={brightness}
            />
            <div className="h-32 sm:h-6" />
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
