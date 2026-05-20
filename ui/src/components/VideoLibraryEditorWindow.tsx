import { useWindowActions, type WindowState } from "@tokimo/sdk";
import { useState } from "react";
import { queryClient } from "../index";
import { getBridge, type ModalBridge } from "../modal-bridge";
import { withProviders } from "../shared/providers";
import VideoLibraryEditor from "../VideoLibraryEditor";

type LibraryEditorBridge = Extract<ModalBridge, { kind: "library-editor" }>;

function VideoLibraryEditorContent({
  win,
  bridge,
}: {
  win: WindowState;
  bridge: LibraryEditorBridge;
}) {
  const { closeWindow } = useWindowActions();
  const videoId =
    typeof win.metadata?.videoId === "string"
      ? win.metadata.videoId
      : undefined;

  return (
    <VideoLibraryEditor
      videoId={videoId}
      onSaved={(savedId) => {
        bridge.onSaved?.(savedId);
        closeWindow(win.id);
      }}
      onDeleted={() => {
        bridge.onDeleted?.();
        closeWindow(win.id);
      }}
      onCancel={() => closeWindow(win.id)}
    />
  );
}

export default function VideoLibraryEditorWindow({
  win,
}: {
  win: WindowState;
}) {
  const bridgeId =
    typeof win.metadata?.bridgeId === "string"
      ? win.metadata.bridgeId
      : undefined;
  const [bridge] = useState(() => (bridgeId ? getBridge(bridgeId) : undefined));

  if (bridge?.kind !== "library-editor") return null;

  return withProviders(
    bridge.ctx,
    queryClient,
    <VideoLibraryEditorContent win={win} bridge={bridge} />,
  );
}
