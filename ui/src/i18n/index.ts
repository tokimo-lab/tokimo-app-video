import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import enUS from "./en-US";
import jaJP from "./ja-JP";
import zhCN from "./zh-CN";

const resources = {
  "zh-CN": { translation: zhCN },
  "en-US": { translation: enUS },
  "ja-JP": { translation: jaJP },
} as const;

export const SUPPORTED_LOCALES = Object.keys(resources);

if (!i18n.isInitialized) {
  i18n.use(initReactI18next).init({
    lng: "zh-CN",
    fallbackLng: "en-US",
    interpolation: { escapeValue: false },
    resources,
  });
}

export default i18n;
