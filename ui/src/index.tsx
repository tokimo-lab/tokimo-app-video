import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { type Dispose, defineApp, RuntimeProvider } from "@tokimo/sdk";
import { ConfigProvider, ToastProvider } from "@tokimo/ui";
import { StrictMode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { I18nextProvider } from "react-i18next";
import { DownloadEngineSettingsSection } from "./components/DownloadEngineSettingsWindow";
import { TmdbSettingsSection } from "./components/TmdbSettingsWindow";
import VideoApp from "./components/VideoApp";
import VideoMenuBar from "./components/VideoMenuBar";
import i18n, { SUPPORTED_LOCALES } from "./i18n";
import { createVideoPlayerExtension } from "./player-extension";
import "./index.css";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: false, refetchOnWindowFocus: false },
  },
});

export default defineApp({
  id: "video",
  manifest: {
    id: "video",
    appName: "dashboard.menu.video",
    icon: "Clapperboard",
    color: "#e11d48",
    windowType: "tokimo-video",
    defaultSize: { width: 1200, height: 800 },
    category: "app",
    appSettings: [
      {
        id: "download-engine",
        title: "media.downloads.engineSettings.title",
        icon: "Settings",
        componentSectionId: "download-engine",
      },
      {
        id: "tmdb",
        title: "media.tmdbSettings.title",
        icon: "Settings",
        componentSectionId: "tmdb",
      },
    ],
  },
  mount(container, ctx): Dispose {
    const applyLocale = (raw: string) => {
      const target = SUPPORTED_LOCALES.includes(raw) ? raw : "en-US";
      if (i18n.language !== target) {
        void i18n.changeLanguage(target);
      }
    };

    applyLocale(ctx.locale);
    const unsubLocale = ctx.shell.subscribeLocale(applyLocale);
    const unregisterPlayerExtension = ctx.shell.player.registerExtension(
      ctx.appId,
      createVideoPlayerExtension(ctx, queryClient),
    );
    const unregisterDownloadEngineSection = ctx.shell.registerAppSection(
      "download-engine",
      DownloadEngineSettingsSection,
    );
    const unregisterTmdbSection = ctx.shell.registerAppSection(
      "tmdb",
      TmdbSettingsSection,
    );

    const root: Root = createRoot(container);
    root.render(
      <StrictMode>
        <I18nextProvider i18n={i18n}>
          <ConfigProvider>
            <ToastProvider>
              <QueryClientProvider client={queryClient}>
                <RuntimeProvider value={ctx}>
                  <VideoMenuBar>
                    <VideoApp />
                  </VideoMenuBar>
                </RuntimeProvider>
              </QueryClientProvider>
            </ToastProvider>
          </ConfigProvider>
        </I18nextProvider>
      </StrictMode>,
    );
    return () => {
      unregisterTmdbSection();
      unregisterDownloadEngineSection();
      unregisterPlayerExtension();
      unsubLocale();
      root.unmount();
    };
  },
});
