import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { type Dispose, defineApp, RuntimeProvider } from "@tokimo/sdk";
import { ConfigProvider, ToastProvider } from "@tokimo/ui";
import { StrictMode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { I18nextProvider } from "react-i18next";
import VideoApp from "./components/VideoApp";
import i18n, { SUPPORTED_LOCALES } from "./i18n";
import "./index.css";

const queryClient = new QueryClient({
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
  },
  mount(container, ctx): Dispose {
    // Sync language with host shell. TODO: subscribe reactively once
    // ctx.shell exposes a locale change event.
    const targetLocale = SUPPORTED_LOCALES.includes(ctx.locale)
      ? ctx.locale
      : "en-US";
    if (i18n.language !== targetLocale) {
      void i18n.changeLanguage(targetLocale);
    }
    const root: Root = createRoot(container);
    root.render(
      <StrictMode>
        <I18nextProvider i18n={i18n}>
          <ConfigProvider>
            <ToastProvider>
              <QueryClientProvider client={queryClient}>
                <RuntimeProvider value={ctx}>
                  <VideoApp />
                </RuntimeProvider>
              </QueryClientProvider>
            </ToastProvider>
          </ConfigProvider>
        </I18nextProvider>
      </StrictMode>,
    );
    return () => root.unmount();
  },
});
