import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { type AppRuntimeCtx, type Dispose, defineApp } from "@tokimo/sdk";
import { ConfigProvider, ToastProvider } from "@tokimo/ui";
import i18n from "i18next";
import { StrictMode, createContext, useContext } from "react";
import { createRoot, type Root } from "react-dom/client";
import { I18nextProvider, initReactI18next } from "react-i18next";
import VideoApp from "./components/VideoApp";
import "./index.css";

// Simple i18n setup (no translations yet, just structure)
i18n.use(initReactI18next).init({
  lng: "en",
  fallbackLng: "en",
  resources: {
    en: { translation: {} },
    zh: { translation: {} },
  },
  interpolation: { escapeValue: false },
});

// Context to pass AppRuntimeCtx to components
const AppCtxContext = createContext<AppRuntimeCtx | null>(null);
export const useAppCtx = () => {
  const ctx = useContext(AppCtxContext);
  if (!ctx) throw new Error("useAppCtx must be used within AppCtxProvider");
  return ctx;
};

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: false, refetchOnWindowFocus: false },
  },
});

export default defineApp({
  id: "tokimo-video",
  manifest: {
    id: "tokimo-video",
    appName: "dashboard.menu.video",
    icon: "Clapperboard",
    color: "#e11d48",
    windowType: "tokimo-video",
    defaultSize: { width: 1200, height: 800 },
    category: "app",
  },
  mount(container, ctx): Dispose {
    const root: Root = createRoot(container);
    root.render(
      <StrictMode>
        <I18nextProvider i18n={i18n}>
          <ConfigProvider>
            <ToastProvider>
              <QueryClientProvider client={queryClient}>
                <AppCtxContext.Provider value={ctx}>
                  <VideoApp />
                </AppCtxContext.Provider>
              </QueryClientProvider>
            </ToastProvider>
          </ConfigProvider>
        </I18nextProvider>
      </StrictMode>,
    );
    return () => root.unmount();
  },
});

