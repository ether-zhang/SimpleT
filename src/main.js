const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// value 写入 system prompt（模型理解的英文名）；code 为 BCP-47，用于本地化显示名
const LANGUAGES = [
  { value: "Chinese", code: "zh" },
  { value: "English", code: "en" },
  { value: "Japanese", code: "ja" },
  { value: "Korean", code: "ko" },
  { value: "French", code: "fr" },
  { value: "German", code: "de" },
  { value: "Spanish", code: "es" },
  { value: "Russian", code: "ru" },
  { value: "Italian", code: "it" },
  { value: "Portuguese", code: "pt" },
  { value: "Arabic", code: "ar" },
  { value: "Thai", code: "th" },
  { value: "Vietnamese", code: "vi" },
];

// 界面语言文案。每项的 _name 是该语言的“母语名”，用于下拉显示。
const I18N = {
  zh: {
    _name: "中文",
    swapTitle: "交换语言",
    inputPh: "输入要翻译的内容…",
    outputPh: "翻译结果",
    translate: "翻译",
    settings: "设置",
    closeTitle: "收起窗口",
    settingsTitle: "设置",
    urlLabel: "模型 URL（OpenAI 格式，以 /v1 结尾）",
    keyLabel: "API Key",
    modelLabel: "模型名称",
    uiLangLabel: "界面语言",
    clearKey: "清除",
    save: "保存",
    back: "返回翻译",
    saved: "已保存 ✓",
    unsaved: "仍有未保存的更改",
    translating: "翻译中…",
  },
  en: {
    _name: "English",
    swapTitle: "Swap languages",
    inputPh: "Enter text to translate…",
    outputPh: "Translation",
    translate: "Translate",
    settings: "Settings",
    closeTitle: "Hide window",
    settingsTitle: "Settings",
    urlLabel: "Model URL (OpenAI format, ends with /v1)",
    keyLabel: "API Key",
    modelLabel: "Model name",
    uiLangLabel: "UI language",
    clearKey: "Clear",
    save: "Save",
    back: "Back",
    saved: "Saved ✓",
    unsaved: "Changes still need to be saved",
    translating: "Translating…",
  },
  ja: {
    _name: "日本語",
    swapTitle: "言語を入れ替え",
    inputPh: "翻訳する内容を入力…",
    outputPh: "翻訳結果",
    translate: "翻訳",
    settings: "設定",
    closeTitle: "ウィンドウを隠す",
    settingsTitle: "設定",
    urlLabel: "モデル URL（OpenAI 形式、/v1 で終わる）",
    keyLabel: "API キー",
    modelLabel: "モデル名",
    uiLangLabel: "表示言語",
    clearKey: "消去",
    save: "保存",
    back: "戻る",
    saved: "保存しました ✓",
    unsaved: "未保存の変更があります",
    translating: "翻訳中…",
  },
  ko: {
    _name: "한국어",
    swapTitle: "언어 교환",
    inputPh: "번역할 내용을 입력…",
    outputPh: "번역 결과",
    translate: "번역",
    settings: "설정",
    closeTitle: "창 숨기기",
    settingsTitle: "설정",
    urlLabel: "모델 URL (OpenAI 형식, /v1로 끝남)",
    keyLabel: "API 키",
    modelLabel: "모델 이름",
    uiLangLabel: "인터페이스 언어",
    clearKey: "지우기",
    save: "저장",
    back: "뒤로",
    saved: "저장됨 ✓",
    unsaved: "저장하지 않은 변경 사항이 있습니다",
    translating: "번역 중…",
  },
  fr: {
    _name: "Français",
    swapTitle: "Inverser les langues",
    inputPh: "Saisir le texte à traduire…",
    outputPh: "Traduction",
    translate: "Traduire",
    settings: "Paramètres",
    closeTitle: "Masquer la fenêtre",
    settingsTitle: "Paramètres",
    urlLabel: "URL du modèle (format OpenAI, se termine par /v1)",
    keyLabel: "Clé API",
    modelLabel: "Nom du modèle",
    uiLangLabel: "Langue de l'interface",
    clearKey: "Effacer",
    save: "Enregistrer",
    back: "Retour",
    saved: "Enregistré ✓",
    unsaved: "Des modifications restent à enregistrer",
    translating: "Traduction…",
  },
  de: {
    _name: "Deutsch",
    swapTitle: "Sprachen tauschen",
    inputPh: "Zu übersetzenden Text eingeben…",
    outputPh: "Übersetzung",
    translate: "Übersetzen",
    settings: "Einstellungen",
    closeTitle: "Fenster ausblenden",
    settingsTitle: "Einstellungen",
    urlLabel: "Modell-URL (OpenAI-Format, endet mit /v1)",
    keyLabel: "API-Schlüssel",
    modelLabel: "Modellname",
    uiLangLabel: "Anzeigesprache",
    clearKey: "Löschen",
    save: "Speichern",
    back: "Zurück",
    saved: "Gespeichert ✓",
    unsaved: "Es gibt noch ungespeicherte Änderungen",
    translating: "Übersetzen…",
  },
  es: {
    _name: "Español",
    swapTitle: "Intercambiar idiomas",
    inputPh: "Escribe el texto a traducir…",
    outputPh: "Traducción",
    translate: "Traducir",
    settings: "Ajustes",
    closeTitle: "Ocultar ventana",
    settingsTitle: "Ajustes",
    urlLabel: "URL del modelo (formato OpenAI, termina en /v1)",
    keyLabel: "Clave API",
    modelLabel: "Nombre del modelo",
    uiLangLabel: "Idioma de la interfaz",
    clearKey: "Borrar",
    save: "Guardar",
    back: "Volver",
    saved: "Guardado ✓",
    unsaved: "Quedan cambios sin guardar",
    translating: "Traduciendo…",
  },
  ru: {
    _name: "Русский",
    swapTitle: "Поменять языки",
    inputPh: "Введите текст для перевода…",
    outputPh: "Перевод",
    translate: "Перевести",
    settings: "Настройки",
    closeTitle: "Скрыть окно",
    settingsTitle: "Настройки",
    urlLabel: "URL модели (формат OpenAI, оканчивается на /v1)",
    keyLabel: "API-ключ",
    modelLabel: "Название модели",
    uiLangLabel: "Язык интерфейса",
    clearKey: "Очистить",
    save: "Сохранить",
    back: "Назад",
    saved: "Сохранено ✓",
    unsaved: "Есть несохранённые изменения",
    translating: "Перевод…",
  },
};

let currentLang = "zh";
let els = {};

function t(key) {
  return (I18N[currentLang] || I18N.zh)[key];
}

// 按语言刷新所有界面文案
function applyLocale(lang) {
  currentLang = I18N[lang] ? lang : "zh";
  document.documentElement.lang = currentLang;
  els.swap.title = t("swapTitle");
  els.input.placeholder = t("inputPh");
  els.output.placeholder = t("outputPh");
  els.translateBtn.textContent = t("translate");
  els.openSettings.textContent = t("settings");
  els.closeFlyout.title = t("closeTitle");
  els.closeFlyout.setAttribute("aria-label", t("closeTitle"));
  els.settingsTitle.textContent = t("settingsTitle");
  els.lblUrl.textContent = t("urlLabel");
  els.lblKey.textContent = t("keyLabel");
  els.cfgKeyClear.textContent = t("clearKey");
  els.lblModel.textContent = t("modelLabel");
  els.lblUiLang.textContent = t("uiLangLabel");
  els.cfgSave.textContent = t("save");
  els.cfgBack.textContent = t("back");
  // A(B) 中的 A 随界面语言变化，需重建两个语种下拉（保留已选值）
  fillLangSelect(els.langA);
  fillLangSelect(els.langB);
}

// 某语言在指定区域设置下的显示名（失败则回退到代码本身）
function displayName(inLocale, code) {
  try {
    return new Intl.DisplayNames([inLocale], { type: "language" }).of(code) || code;
  } catch {
    return code;
  }
}

// 「当前界面语言译名 (该语言母语名)」，两者相同时只显示一个
function langLabel(code) {
  const a = displayName(currentLang, code);
  const b = displayName(code, code);
  return a === b ? a : `${a} (${b})`;
}

function fillLangSelect(sel) {
  const prev = sel.value;
  sel.innerHTML = "";
  for (const l of LANGUAGES) {
    const o = document.createElement("option");
    o.value = l.value;
    o.textContent = langLabel(l.code);
    sel.appendChild(o);
  }
  if (prev) sel.value = prev;
}

// UI 语言下拉：每项用该语言的母语名显示
function fillUiLangSelect(sel) {
  sel.innerHTML = "";
  for (const [code, dict] of Object.entries(I18N)) {
    const o = document.createElement("option");
    o.value = code;
    o.textContent = dict._name;
    sel.appendChild(o);
  }
}

// 收起动画时长，需与 styles.css 中 .card 的 transition 时长保持一致
const CLOSE_MS = 280;
let hideTimer = null;
let firstOpenFrame = null;
let secondOpenFrame = null;
let translationInFlight = false;
let apiKeyChanged = false;
let apiKeyConfigured = false;
let configLoadError = "";
let flyoutGeneration = 0;
let closeRequested = false;
let currentPage = "translate";
let focusFrame = null;
let apiKeyRevision = 0;
let configSaveInFlight = false;
let configWriteQueue = Promise.resolve();
let configStatusTimer = null;

function updateApiKeyUI() {
  els.cfgKey.placeholder = !apiKeyChanged && apiKeyConfigured ? "••••••••" : "sk-…";
  const hasKey = apiKeyChanged ? Boolean(els.cfgKey.value.trim()) : apiKeyConfigured;
  els.cfgKeyClear.classList.toggle("hidden", !hasKey);
}

function showConfigStatus(message, clearAfter = 0) {
  clearTimeout(configStatusTimer);
  configStatusTimer = null;
  els.cfgStatus.textContent = message;
  if (clearAfter) {
    configStatusTimer = setTimeout(() => {
      configStatusTimer = null;
      els.cfgStatus.textContent = "";
    }, clearAfter);
  }
}

function queueConfigWrite(command, args) {
  const pending = configWriteQueue.then(() => invoke(command, args));
  configWriteQueue = pending.catch(() => {});
  return pending;
}

function cancelPageFocus() {
  if (focusFrame !== null) cancelAnimationFrame(focusFrame);
  focusFrame = null;
}

function requestClose() {
  if (closeRequested) return;
  closeRequested = true;
  const generation = flyoutGeneration;
  invoke("request_hide", { generation }).catch((error) => {
    if (generation === flyoutGeneration) {
      closeRequested = false;
      els.status.textContent = String(error);
    }
  });
}

function cancelSlideInFrames() {
  if (firstOpenFrame !== null) cancelAnimationFrame(firstOpenFrame);
  if (secondOpenFrame !== null) cancelAnimationFrame(secondOpenFrame);
  firstOpenFrame = null;
  secondOpenFrame = null;
}

function setFlyoutOrigin(origin) {
  const fromTop = origin === "top";
  els.card.classList.toggle("from-top", fromTop);
  els.card.classList.toggle("from-bottom", !fromTop);
}

function prepareSlideIn(origin) {
  els.card.classList.add("no-transition");
  els.card.classList.remove("show");
  setFlyoutOrigin(origin);
  els.card.offsetHeight;
  els.card.classList.remove("no-transition");
}

// 卡片从下方滑入（打开）
function slideIn(origin) {
  closeRequested = false;
  clearTimeout(hideTimer);
  hideTimer = null;
  cancelSlideInFrames();
  prepareSlideIn(origin);
  // 双 rAF：先让“隐藏态”绘制一帧，再触发过渡，避免直接闪现
  firstOpenFrame = requestAnimationFrame(() => {
    firstOpenFrame = null;
    secondOpenFrame = requestAnimationFrame(() => {
      secondOpenFrame = null;
      els.card.classList.add("show");
    });
  });
}

// 卡片向下滑出（收起），动画结束后再真正隐藏窗口——这样能看到下滑过程
function slideOutThenHide(generation) {
  if (generation !== flyoutGeneration || hideTimer !== null) return;
  closeRequested = true;
  cancelPageFocus();
  cancelSlideInFrames();
  els.card.classList.remove("show");
  hideTimer = setTimeout(() => {
    hideTimer = null;
    invoke("commit_hide", { generation }).catch((e) => {
      if (generation === flyoutGeneration) els.status.textContent = String(e);
    });
  }, CLOSE_MS + 40);
}

function showPage(page) {
  const isSettings = page === "settings";
  currentPage = isSettings ? "settings" : "translate";
  els.pageTranslate.classList.toggle("hidden", isSettings);
  els.pageSettings.classList.toggle("hidden", !isSettings);
  // 显示后立刻聚焦输入框，让中文输入法候选框贴着光标出现（修复其跑到左上角的问题）
  cancelPageFocus();
  focusFrame = requestAnimationFrame(() => {
    focusFrame = null;
    (isSettings ? els.cfgUrl : els.input).focus();
  });
}

async function doTranslate() {
  if (translationInFlight) return;
  const text = els.input.value;
  if (!text.trim()) {
    els.output.value = "";
    return;
  }
  const langA = els.langA.value;
  const langB = els.langB.value;
  const isCurrentInput = () =>
    els.input.value === text && els.langA.value === langA && els.langB.value === langB;
  const progressMessage = t("translating");
  translationInFlight = true;
  els.status.textContent = progressMessage;
  els.translateBtn.disabled = true;
  try {
    const result = await invoke("translate", {
      text,
      langA,
      langB,
    });
    if (!isCurrentInput()) return;
    els.output.value = result;
    els.status.textContent = "";
  } catch (e) {
    if (!isCurrentInput()) return;
    els.output.value = "";
    els.status.textContent = String(e);
  } finally {
    if (!isCurrentInput() && els.status.textContent === progressMessage) {
      els.status.textContent = "";
    }
    translationInFlight = false;
    els.translateBtn.disabled = false;
  }
}

async function loadConfigIntoUI() {
  const cfg = await invoke("load_config");
  els.cfgUrl.value = cfg.base_url || "";
  apiKeyConfigured = Boolean(cfg.api_key_configured);
  apiKeyChanged = false;
  els.cfgKey.value = "";
  updateApiKeyUI();
  els.cfgModel.value = cfg.model || "";
  els.langA.value = cfg.lang_a || "Chinese";
  els.langB.value = cfg.lang_b || "English";
  els.cfgUiLang.value = cfg.ui_lang || "zh";
  applyLocale(els.cfgUiLang.value);
  configLoadError = cfg.load_error || "";
  showConfigStatus(configLoadError);
  els.status.textContent = configLoadError;
}

async function saveConfig() {
  if (configSaveInFlight) return;
  const keyRevision = apiKeyRevision;
  const config = {
    base_url: els.cfgUrl.value.trim(),
    api_key: apiKeyChanged ? els.cfgKey.value.trim() : null,
    model: els.cfgModel.value.trim(),
    lang_a: els.langA.value,
    lang_b: els.langB.value,
    ui_lang: els.cfgUiLang.value,
  };
  configSaveInFlight = true;
  els.cfgSave.disabled = true;
  try {
    await queueConfigWrite("save_config", { config });
    const manualDraftUnchanged = apiKeyRevision === keyRevision
      && els.cfgUrl.value.trim() === config.base_url
      && els.cfgModel.value.trim() === config.model;
    if (config.api_key !== null) {
      apiKeyConfigured = Boolean(config.api_key);
    }
    if (apiKeyRevision === keyRevision) {
      apiKeyChanged = false;
      els.cfgKey.value = "";
    }
    updateApiKeyUI();
    showConfigStatus(t(manualDraftUnchanged ? "saved" : "unsaved"), manualDraftUnchanged ? 1500 : 0);
    if (configLoadError && els.status.textContent === configLoadError) {
      els.status.textContent = "";
    }
    configLoadError = "";
  } catch (e) {
    showConfigStatus(String(e));
  } finally {
    configSaveInFlight = false;
    els.cfgSave.disabled = false;
  }
}

// 界面语言切换后立即持久化
async function persistUiLang() {
  try {
    await queueConfigWrite("save_ui_lang", { uiLang: els.cfgUiLang.value });
  } catch (e) {
    showConfigStatus(String(e));
  }
}

// 语言切换后，把当前选择持久化，方便下次启动恢复
async function persistLangs() {
  try {
    await queueConfigWrite("save_languages", {
      langA: els.langA.value,
      langB: els.langB.value,
    });
  } catch (e) {
    els.status.textContent = String(e);
  }
}

window.addEventListener("DOMContentLoaded", async () => {
  els = {
    card: document.querySelector(".card"),
    closeFlyout: document.querySelector("#close-flyout"),
    pageTranslate: document.querySelector("#page-translate"),
    pageSettings: document.querySelector("#page-settings"),
    langA: document.querySelector("#lang-a"),
    langB: document.querySelector("#lang-b"),
    swap: document.querySelector("#swap"),
    input: document.querySelector("#input"),
    output: document.querySelector("#output"),
    translateBtn: document.querySelector("#translate-btn"),
    status: document.querySelector("#status"),
    cfgUrl: document.querySelector("#cfg-url"),
    cfgKey: document.querySelector("#cfg-key"),
    cfgKeyClear: document.querySelector("#cfg-key-clear"),
    cfgModel: document.querySelector("#cfg-model"),
    cfgUiLang: document.querySelector("#cfg-ui-lang"),
    openSettings: document.querySelector("#open-settings"),
    cfgSave: document.querySelector("#cfg-save"),
    cfgBack: document.querySelector("#cfg-back"),
    cfgStatus: document.querySelector("#cfg-status"),
    settingsTitle: document.querySelector("#settings-title"),
    lblUrl: document.querySelector("#lbl-url"),
    lblKey: document.querySelector("#lbl-key"),
    lblModel: document.querySelector("#lbl-model"),
    lblUiLang: document.querySelector("#lbl-uilang"),
  };

  fillLangSelect(els.langA);
  fillLangSelect(els.langB);
  fillUiLangSelect(els.cfgUiLang);
  els.langA.value = "Chinese";
  els.langB.value = "English";
  els.cfgUiLang.value = "zh";
  applyLocale("zh");
  updateApiKeyUI();

  await Promise.all([
    listen("navigate", (e) => {
      const payload = e.payload;
      const page = typeof payload === "string" ? payload : payload?.page;
      const origin = typeof payload === "string" ? "bottom" : payload?.origin;
      flyoutGeneration = payload?.generation ?? flyoutGeneration;
      showPage(page || "translate");
      slideIn(origin || "bottom");
    }),
    listen("flyout-hide", (e) => slideOutThenHide(e.payload?.generation)),
  ]);

  els.translateBtn.addEventListener("click", doTranslate);
  els.closeFlyout.addEventListener("click", requestClose);
  els.cfgKey.addEventListener("input", () => {
    apiKeyChanged = true;
    apiKeyRevision++;
    updateApiKeyUI();
  });
  els.cfgKeyClear.addEventListener("click", () => {
    apiKeyChanged = true;
    apiKeyRevision++;
    els.cfgKey.value = "";
    updateApiKeyUI();
  });
  // Ctrl+Enter 快速翻译
  els.input.addEventListener("keydown", (e) => {
    if (e.isComposing || e.keyCode === 229) return;
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      doTranslate();
    }
  });

  els.swap.addEventListener("click", async () => {
    const a = els.langA.value;
    els.langA.value = els.langB.value;
    els.langB.value = a;
    // 同时交换输入/输出，方便反向确认
    const t = els.input.value;
    els.input.value = els.output.value;
    els.output.value = t;
    await persistLangs();
  });

  els.langA.addEventListener("change", persistLangs);
  els.langB.addEventListener("change", persistLangs);

  els.openSettings.addEventListener("click", () => showPage("settings"));
  els.cfgSave.addEventListener("click", saveConfig);
  els.cfgBack.addEventListener("click", () => showPage("translate"));

  // 切换界面语言：立即刷新文案并持久化
  els.cfgUiLang.addEventListener("change", async () => {
    applyLocale(els.cfgUiLang.value);
    await persistUiLang();
  });

  // Esc 收起浮窗（带下滑动画）
  document.addEventListener("keydown", (e) => {
    if (e.isComposing || e.keyCode === 229) return;
    if (e.key === "Escape") requestClose();
  });

  try {
    await loadConfigIntoUI();
  } catch (error) {
    configLoadError = String(error);
    showConfigStatus(configLoadError);
    els.status.textContent = configLoadError;
  }
  showPage(currentPage);
  // Only acknowledge readiness after listeners, controls and configuration are
  // initialized. Rust then delivers any tray navigation requested during startup.
  try {
    await invoke("frontend_ready");
  } catch (error) {
    els.status.textContent = String(error);
  }
});
