import { type QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { type AppRuntimeCtx, RuntimeProvider } from "@tokimo/sdk";
import { ConfigProvider, ToastProvider } from "@tokimo/ui";
import type { ReactNode } from "react";
import { I18nextProvider } from "react-i18next";
import i18n from "../i18n";

export function withProviders(
  ctx: AppRuntimeCtx,
  queryClient: QueryClient,
  node: ReactNode,
): ReactNode {
  return (
    <I18nextProvider i18n={i18n}>
      <ConfigProvider>
        <ToastProvider>
          <QueryClientProvider client={queryClient}>
            <RuntimeProvider value={ctx}>{node}</RuntimeProvider>
          </QueryClientProvider>
        </ToastProvider>
      </ConfigProvider>
    </I18nextProvider>
  );
}
