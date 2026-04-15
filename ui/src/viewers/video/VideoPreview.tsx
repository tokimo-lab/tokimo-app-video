/**
 * VideoPreview — Simple standalone video player for embedding.
 *
 * Uses native HTML5 video controls. For window-level features like
 * position saving and sibling navigation, use VideoViewer instead.
 */

interface VideoPreviewProps {
  src: string;
  className?: string;
}

export function VideoPreview({ src, className }: VideoPreviewProps) {
  return (
    <div
      className={`flex items-center justify-center bg-black ${className ?? ""}`}
    >
      <video src={src} controls className="max-h-full max-w-full">
        <track kind="captions" />
      </video>
    </div>
  );
}
