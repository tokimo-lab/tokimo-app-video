import { Avatar, Spin } from "@tokimo/ui";
import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";
import { api } from "../shell-shim/api";
import { parseUserAgent } from "../shell-shim/lib";
import { useAuth, useDateFormat } from "../shell-shim/system";

dayjs.extend(relativeTime);

interface WatchHistoryTableProps {
  videoItemId?: string;
  episodeId?: string;
  tvShowId?: string;
  /**
   * Called when user clicks "继续播放". The parent is responsible for
   * looking up the file and calling play() — the component only passes
   * back what it knows: fileId, resume position, and the history record id.
   */
  onResumePlay?: (fileId: string, position: number, historyId: string) => void;
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
  tvShowId,
  onResumePlay,
}: WatchHistoryTableProps) {
  const { formatLong } = useDateFormat();
  const { user } = useAuth();
  const { data, isLoading, refetch } = api.playback.watchHistory.useQuery(
    { videoItemId, episodeId, tvShowId, limit: 20 },
    { enabled: !!(videoItemId || episodeId || tvShowId) },
  );
  const deleteMutation = api.playback.deleteWatchHistory.useMutation({
    onSuccess: () => refetch(),
  });

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
          <tr className="border-b border-border-base text-left text-xs text-fg-muted">
            <th className="py-2 pr-4 font-medium">时间</th>
            {tvShowId && <th className="py-2 pr-4 font-medium">剧集</th>}
            <th className="py-2 pr-4 font-medium">用户</th>
            <th className="py-2 pr-4 font-medium">播放位置</th>
            <th className="py-2 pr-4 font-medium">进度</th>
            <th className="py-2 pr-4 font-medium">客户端</th>
            <th className="py-2 pr-4 font-medium">状态</th>
            <th className="py-2 font-medium">操作</th>
          </tr>
        </thead>
        <tbody>
          {data.map((item) => {
            const pct = progressPercent(item);
            const ua = parseUserAgent(item.userAgent);
            const canContinue = !item.completed && !!item.fileId;
            const episodeLabel =
              item.seasonNumber != null && item.episodeNumber != null
                ? `S${item.seasonNumber}E${item.episodeNumber}`
                : item.episodeNumber != null
                  ? `第 ${item.episodeNumber} 集`
                  : null;
            return (
              <tr
                key={item.id}
                className="border-b border-border-base/50 last:border-0"
              >
                <td className="py-2 pr-4">
                  <span title={formatLong(item.startedAt)}>
                    {dayjs(item.startedAt).fromNow()}
                  </span>
                </td>
                {tvShowId && (
                  <td className="py-2 pr-4 font-mono text-xs text-fg-secondary">
                    {episodeLabel ?? "—"}
                  </td>
                )}
                <td className="py-2 pr-4">
                  {item.userName ? (
                    <div className="flex items-center gap-2">
                      <Avatar user={user} size={22} />
                      <span className="text-fg-secondary">{item.userName}</span>
                    </div>
                  ) : (
                    "—"
                  )}
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
                <td
                  className="py-2 pr-4 text-fg-muted"
                  title={item.userAgent ?? undefined}
                >
                  {item.userAgent ? ua.summary : (item.clientName ?? "—")}
                </td>
                <td className="py-2 pr-4">
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
                <td className="py-2">
                  <div className="flex items-center gap-1">
                    {canContinue && onResumePlay && (
                      <button
                        type="button"
                        className="cursor-pointer rounded px-2 py-1 text-xs text-[var(--accent)] transition-colors hover:bg-[var(--accent)]/10"
                        onClick={() =>
                          onResumePlay(item.fileId!, item.position, item.id)
                        }
                      >
                        继续播放
                      </button>
                    )}
                    <button
                      type="button"
                      className="cursor-pointer rounded px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-red-500/10 hover:text-red-400"
                      onClick={() => deleteMutation.mutate(item.id)}
                    >
                      删除
                    </button>
                  </div>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
