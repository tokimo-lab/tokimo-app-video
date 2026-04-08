import { Modal, Spin } from "@tokiomo/components";
import { lazy, Suspense } from "react";

const VideoSettingsPage = lazy(
  () => import("@/apps/settings/admin/VideoSettingsPage"),
);

interface VideoSettingsModalProps {
  open: boolean;
  onClose: () => void;
}

export default function VideoSettingsModal({
  open,
  onClose,
}: VideoSettingsModalProps) {
  return (
    <Modal
      open={open}
      onCancel={onClose}
      title="TokimoVideo 设置"
      footer={null}
      width={800}
      destroyOnClose
      styles={{ body: { padding: 0 } }}
    >
      <div className="h-[560px]">
        <Suspense
          fallback={
            <div className="flex h-full items-center justify-center">
              <Spin />
            </div>
          }
        >
          <VideoSettingsPage />
        </Suspense>
      </div>
    </Modal>
  );
}
