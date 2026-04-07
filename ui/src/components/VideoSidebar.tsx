import { AppSidebar } from "@tokiomo/components";
import type { VideoOutput } from "@/generated/rust-api";
import { AppIcon } from "@/shared/components/icons";

export default function VideoSidebar({
  categories,
  activeId,
  onSelect,
}: {
  categories: VideoOutput[];
  activeId: string | null;
  onSelect: (id: string) => void;
}) {
  const sections = [
    {
      items: categories.map((cat) => ({
        key: cat.id,
        icon: <AppIcon icon={cat.icon} color={cat.color} size={20} />,
        label: cat.name,
        extra:
          cat.itemCount > 0 ? (
            <span className="text-[10px] tabular-nums text-fg-muted">
              {cat.itemCount}
            </span>
          ) : undefined,
      })),
    },
  ];

  return (
    <AppSidebar
      sections={sections}
      activeKey={activeId ?? undefined}
      onSelect={onSelect}
    />
  );
}
