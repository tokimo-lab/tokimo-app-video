import { Spin } from "@tokiomo/components";
import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";
import { api } from "@/generated/rust-api";

dayjs.extend(relativeTime);

interface WatchHistoryTableProps {
  videoItemId?: string;
  episodeId?: string;
}

function formatDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
    : `${m}:${String(s).padStart(2, "0")}`;
}

function progressPercent(item: {
  position: number;
  duration: number | null;
}): number | null {
  if (!item.duration || item.duration === 0) return null;
  return Math.round((item.position / item.duration) * 100);
}

export function WatchHistoryTable({
  videoItemId,
  episodeId,
}: WatchHistoryTableProps) {
  const { data, isLoading } = api.playback.watchHistory.useQuery(
    { videoItemId, episodeId, limit: 20 },
    { enabled: !!(videoItemId || episodeId) },
  );

  if (isLoading) {
    return (
      <div className="flex justify-center py-8">
        <Spin />
      </div>
    );
  }

  if (!data?.length) {
    return (
      <p className="py-6 text-center text-sm text-fg-muted">暂无播放记录</p>
    );
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-[var(--border-base)] text-left text-xs text-fg-muted">
            <th className="py-2 pr-4 font-medium">时间</th>
            <th className="py-2 pr-4 font-medium">播放位置</th>
            <th className="py-2 pr-4 font-medium">进度</th>
            <th className="py-2 pr-4 font-medium">客户端</th>
            <th className="py-2 font-medium">状态</th>
          </tr>
        </thead>
        <tbody>
          {data.map((item) => {
            const pct = progressPercent(item);
            return (
              <tr
                key={item.id}
                className="border-b border-[var(--border-base)]/50 last:border-0"
              >
                <td className="py-2 pr-4">
                  <span
                    title={dayjs(item.startedAt).format("YYYY-MM-DD HH:mm:ss")}
                  >
                    {dayjs(item.startedAt).fromNow()}
                  </span>
                </td>
                <td className="py-2 pr-4 font-mono">
                  {formatDuration(item.position)}
                  {item.duration ? ` / ${formatDuration(item.duration)}` : ""}
                </td>
                <td className="py-2 pr-4">
                  {pct !== null ? (
                    <div className="flex items-center gap-2">
                      <div className="h-1.5 w-20 overflow-hidden rounded-full bg-gray-700">
                        <div
                          className="h-full rounded-full bg-[var(--accent)]"
                          style={{ width: `${pct}%` }}
                        />
                      </div>
                      <span className="text-xs text-fg-muted">{pct}%</span>
                    </div>
                  ) : (
                    "—"
                  )}
                </td>
                <td className="py-2 pr-4 text-fg-muted">
                  {item.clientName ?? "—"}
                </td>
                <td className="py-2">
                  {item.completed ? (
                    <span className="rounded-full bg-green-500/20 px-2 py-0.5 text-xs text-green-400">
                      已看完
                    </span>
                  ) : (
                    <span className="rounded-full bg-blue-500/20 px-2 py-0.5 text-xs text-blue-400">
                      进行中
                    </span>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
