import enUS from "./locales/en-US.ts";
import zhCN from "./locales/zh-CN.ts";

export const supportedLanguages = ["zh-CN", "en-US"] as const;
export type Language = (typeof supportedLanguages)[number];
export type TranslationKey = keyof typeof zhCN;
export type TranslationVariables = Record<string, string | number>;

const STORAGE_KEY = "rust-yu.language";
const translations: Record<Language, Record<TranslationKey, string>> = {
  "zh-CN": zhCN,
  "en-US": enUS,
};

function detectLanguage(): Language {
  const storage = typeof globalThis.localStorage !== "undefined" && typeof globalThis.localStorage.getItem === "function"
    ? globalThis.localStorage
    : null;
  if (!storage) return "zh-CN";
  const saved = storage.getItem(STORAGE_KEY);
  if (saved === "zh-CN" || saved === "en-US") return saved;
  return typeof navigator !== "undefined" && navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
}

let currentLanguage = detectLanguage();
if (typeof document !== "undefined") document.documentElement.lang = currentLanguage;

export function getLanguage(): Language {
  return currentLanguage;
}

export function setLanguage(language: Language): void {
  if (language === currentLanguage) return;
  if (typeof globalThis.localStorage !== "undefined" && typeof globalThis.localStorage.setItem === "function") {
    globalThis.localStorage.setItem(STORAGE_KEY, language);
  }
  currentLanguage = language;
  if (typeof document !== "undefined") document.documentElement.lang = language;
  if (typeof window !== "undefined") window.location.reload();
}

export function t(key: TranslationKey, variables: TranslationVariables = {}): string {
  const template = translations[currentLanguage][key];
  return template.replace(/\{(\w+)\}/g, (placeholder, name: string) => {
    const value = variables[name];
    return value === undefined ? placeholder : String(value);
  });
}
