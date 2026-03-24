/**
 * VideoContent — Window content adapter for video files.
 */

import type { WindowState } from "../../../contexts/WindowManagerContext";
import { buildFileUrl } from "../../file-manager/types";
import { VideoPlayer } from "../../player/VideoPlayer";
import { buildSshFileUrl } from "../file-url";

export default function VideoContent({ win }: { win: WindowState }) {
  const filePath = win.metadata.filePath ?? "";
  const fileSystemId = win.metadata.fileSystemId ?? "";

  if (win.sourceType === "player") {
    return (
      <div className="relative h-full bg-black">
        <VideoPlayer />
      </div>
    );
  }

  const videoSrc =
    buildFileUrl(filePath, fileSystemId) ??
    buildSshFileUrl(win.metadata.sshTerminalId, filePath);

  return (
    <div className="flex h-full items-center justify-center bg-black p-2">
      {videoSrc && (
        // biome-ignore lint/a11y/useMediaCaption: file preview
        <video
          src={videoSrc}
          controls
          className="max-h-full max-w-full rounded"
        />
      )}
    </div>
  );
}
