const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const isMac = /Mac/i.test(navigator.userAgentData?.platform || navigator.platform || "");
const translateShortcut = isMac ? "⌘ + ↵" : "Ctrl + ↵";
// The clipboard plugin serializes arboard's non-text/empty result as this string.
const NON_TEXT_CLIPBOARD = "The clipboard contents were not available in the requested format or the clipboard is empty.";

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
    appearanceLabel: "外观",
    appearanceDefault: "默认",
    appearanceGlass: "液态玻璃",
    appearanceError: "外观设置失败",
    history: "历史记录",
    historyHint: "向上查看更早记录",
    historyEmpty: "还没有历史记录，完成翻译后会自动保存在这里",
    historyLoading: "加载中…",
    historyMore: "加载更早记录",
    historyLoadError: "历史记录加载失败",
    historySaveError: "本次翻译未能保存到历史记录",
    retry: "重试",
    sourceLanguage: "源语言",
    targetLanguage: "目标语言",
    pasteInput: "追加粘贴",
    clearInput: "清除输入",
    copyOutput: "复制结果",
    clearOutput: "清除结果",
    copied: "已复制",
    pasteError: "无法读取剪贴板",
    copyError: "复制失败",
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
    modelsIdle: "填写服务 URL 后获取可用模型",
    modelsInvalidUrl: "请输入有效的 HTTP / HTTPS 服务 URL",
    modelsLoading: "正在检查服务并获取模型…",
    modelsReady: "服务可用 · {count} 个模型",
    modelsAuth: "认证失败，请检查 API Key",
    modelsNetwork: "无法连接服务，请检查 URL 或网络",
    modelsTimeout: "连接超时，请重试",
    modelsHttp: "获取模型失败（HTTP {status}）",
    modelsInvalidResponse: "服务未返回有效的模型列表",
    modelsEmpty: "服务可连接，但未提供可用模型",
    modelConnect: "连接服务后选择模型",
    modelSelect: "请选择模型",
    modelSearch: "搜索模型…",
    modelsNoMatch: "没有匹配的模型",
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
    appearanceLabel: "Appearance",
    appearanceDefault: "Default",
    appearanceGlass: "Liquid Glass",
    appearanceError: "Unable to update appearance",
    history: "History",
    historyHint: "Scroll up for older translations",
    historyEmpty: "Completed translations will appear here",
    historyLoading: "Loading…",
    historyMore: "Load older translations",
    historyLoadError: "Unable to load history",
    historySaveError: "This translation could not be saved to history",
    retry: "Retry",
    sourceLanguage: "Source language",
    targetLanguage: "Target language",
    pasteInput: "Paste at end",
    clearInput: "Clear input",
    copyOutput: "Copy result",
    clearOutput: "Clear result",
    copied: "Copied",
    pasteError: "Unable to read clipboard",
    copyError: "Copy failed",
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
    modelsIdle: "Enter a service URL to load available models",
    modelsInvalidUrl: "Enter a valid HTTP / HTTPS service URL",
    modelsLoading: "Checking the service and loading models…",
    modelsReady: "Service available · {count} models",
    modelsAuth: "Authentication failed. Check your API key",
    modelsNetwork: "Cannot connect. Check the URL or network",
    modelsTimeout: "Connection timed out. Please retry",
    modelsHttp: "Unable to load models (HTTP {status})",
    modelsInvalidResponse: "The service did not return a valid model list",
    modelsEmpty: "Service connected, but no models are available",
    modelConnect: "Connect to select a model",
    modelSelect: "Select a model",
    modelSearch: "Search models…",
    modelsNoMatch: "No matching models",
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
    appearanceLabel: "外観",
    appearanceDefault: "デフォルト",
    appearanceGlass: "Liquid Glass",
    appearanceError: "外観を変更できません",
    history: "履歴",
    historyHint: "上にスクロールして過去の翻訳を表示",
    historyEmpty: "翻訳が完了すると、ここに保存されます",
    historyLoading: "読み込み中…",
    historyMore: "過去の翻訳を読み込む",
    historyLoadError: "履歴を読み込めません",
    historySaveError: "今回の翻訳を履歴に保存できませんでした",
    retry: "再試行",
    sourceLanguage: "翻訳元の言語",
    targetLanguage: "翻訳先の言語",
    pasteInput: "末尾に貼り付け",
    clearInput: "入力を消去",
    copyOutput: "結果をコピー",
    clearOutput: "結果を消去",
    copied: "コピーしました",
    pasteError: "クリップボードを読み取れません",
    copyError: "コピーに失敗しました",
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
    modelsIdle: "サービス URL を入力するとモデルを取得します",
    modelsInvalidUrl: "有効な HTTP / HTTPS URL を入力してください",
    modelsLoading: "サービスを確認してモデルを取得中…",
    modelsReady: "接続可能 · {count} モデル",
    modelsAuth: "認証に失敗しました。API キーを確認してください",
    modelsNetwork: "接続できません。URL やネットワークを確認してください",
    modelsTimeout: "接続がタイムアウトしました。再試行してください",
    modelsHttp: "モデルを取得できません（HTTP {status}）",
    modelsInvalidResponse: "有効なモデル一覧が返されませんでした",
    modelsEmpty: "接続できましたが、利用可能なモデルがありません",
    modelConnect: "接続後にモデルを選択",
    modelSelect: "モデルを選択",
    modelSearch: "モデルを検索…",
    modelsNoMatch: "該当するモデルがありません",
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
    appearanceLabel: "모양",
    appearanceDefault: "기본",
    appearanceGlass: "리퀴드 글래스",
    appearanceError: "모양을 변경할 수 없습니다",
    history: "기록",
    historyHint: "위로 스크롤하여 이전 번역 보기",
    historyEmpty: "완료된 번역이 여기에 저장됩니다",
    historyLoading: "불러오는 중…",
    historyMore: "이전 번역 불러오기",
    historyLoadError: "기록을 불러올 수 없습니다",
    historySaveError: "이번 번역을 기록에 저장하지 못했습니다",
    retry: "다시 시도",
    sourceLanguage: "원본 언어",
    targetLanguage: "대상 언어",
    pasteInput: "끝에 붙여넣기",
    clearInput: "입력 지우기",
    copyOutput: "결과 복사",
    clearOutput: "결과 지우기",
    copied: "복사됨",
    pasteError: "클립보드를 읽을 수 없습니다",
    copyError: "복사 실패",
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
    modelsIdle: "서비스 URL을 입력하면 모델을 불러옵니다",
    modelsInvalidUrl: "유효한 HTTP / HTTPS URL을 입력하세요",
    modelsLoading: "서비스 확인 및 모델 불러오는 중…",
    modelsReady: "서비스 사용 가능 · 모델 {count}개",
    modelsAuth: "인증에 실패했습니다. API 키를 확인하세요",
    modelsNetwork: "연결할 수 없습니다. URL 또는 네트워크를 확인하세요",
    modelsTimeout: "연결 시간이 초과되었습니다. 다시 시도하세요",
    modelsHttp: "모델을 불러올 수 없습니다 (HTTP {status})",
    modelsInvalidResponse: "유효한 모델 목록이 반환되지 않았습니다",
    modelsEmpty: "연결되었으나 사용 가능한 모델이 없습니다",
    modelConnect: "연결 후 모델 선택",
    modelSelect: "모델 선택",
    modelSearch: "모델 검색…",
    modelsNoMatch: "일치하는 모델이 없습니다",
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
    appearanceLabel: "Apparence",
    appearanceDefault: "Par défaut",
    appearanceGlass: "Liquid Glass",
    appearanceError: "Impossible de modifier l’apparence",
    history: "Historique",
    historyHint: "Défiler vers le haut pour les anciennes traductions",
    historyEmpty: "Les traductions terminées apparaîtront ici",
    historyLoading: "Chargement…",
    historyMore: "Charger les traductions précédentes",
    historyLoadError: "Impossible de charger l’historique",
    historySaveError: "Cette traduction n’a pas pu être enregistrée",
    retry: "Réessayer",
    sourceLanguage: "Langue source",
    targetLanguage: "Langue cible",
    pasteInput: "Coller à la fin",
    clearInput: "Effacer la saisie",
    copyOutput: "Copier le résultat",
    clearOutput: "Effacer le résultat",
    copied: "Copié",
    pasteError: "Lecture du presse-papiers impossible",
    copyError: "Échec de la copie",
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
    modelsIdle: "Saisissez l’URL du service pour charger les modèles",
    modelsInvalidUrl: "Saisissez une URL HTTP / HTTPS valide",
    modelsLoading: "Vérification du service et chargement des modèles…",
    modelsReady: "Service disponible · {count} modèles",
    modelsAuth: "Échec de l’authentification. Vérifiez la clé API",
    modelsNetwork: "Connexion impossible. Vérifiez l’URL ou le réseau",
    modelsTimeout: "Délai de connexion dépassé. Réessayez",
    modelsHttp: "Chargement des modèles impossible (HTTP {status})",
    modelsInvalidResponse: "Le service n’a pas renvoyé de liste de modèles valide",
    modelsEmpty: "Service connecté, mais aucun modèle disponible",
    modelConnect: "Connectez-vous pour choisir un modèle",
    modelSelect: "Choisir un modèle",
    modelSearch: "Rechercher un modèle…",
    modelsNoMatch: "Aucun modèle correspondant",
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
    appearanceLabel: "Darstellung",
    appearanceDefault: "Standard",
    appearanceGlass: "Liquid Glass",
    appearanceError: "Darstellung kann nicht geändert werden",
    history: "Verlauf",
    historyHint: "Für ältere Übersetzungen nach oben scrollen",
    historyEmpty: "Abgeschlossene Übersetzungen erscheinen hier",
    historyLoading: "Wird geladen…",
    historyMore: "Ältere Übersetzungen laden",
    historyLoadError: "Verlauf kann nicht geladen werden",
    historySaveError: "Diese Übersetzung konnte nicht gespeichert werden",
    retry: "Erneut versuchen",
    sourceLanguage: "Ausgangssprache",
    targetLanguage: "Zielsprache",
    pasteInput: "Am Ende einfügen",
    clearInput: "Eingabe leeren",
    copyOutput: "Ergebnis kopieren",
    clearOutput: "Ergebnis leeren",
    copied: "Kopiert",
    pasteError: "Zwischenablage kann nicht gelesen werden",
    copyError: "Kopieren fehlgeschlagen",
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
    modelsIdle: "Service-URL eingeben, um Modelle zu laden",
    modelsInvalidUrl: "Eine gültige HTTP-/HTTPS-URL eingeben",
    modelsLoading: "Service wird geprüft, Modelle werden geladen…",
    modelsReady: "Service verfügbar · {count} Modelle",
    modelsAuth: "Authentifizierung fehlgeschlagen. API-Schlüssel prüfen",
    modelsNetwork: "Keine Verbindung. URL oder Netzwerk prüfen",
    modelsTimeout: "Zeitüberschreitung. Bitte erneut versuchen",
    modelsHttp: "Modelle konnten nicht geladen werden (HTTP {status})",
    modelsInvalidResponse: "Der Service lieferte keine gültige Modellliste",
    modelsEmpty: "Service verbunden, aber keine Modelle verfügbar",
    modelConnect: "Verbinden, um ein Modell auszuwählen",
    modelSelect: "Modell auswählen",
    modelSearch: "Modelle suchen…",
    modelsNoMatch: "Keine passenden Modelle",
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
    appearanceLabel: "Apariencia",
    appearanceDefault: "Predeterminada",
    appearanceGlass: "Liquid Glass",
    appearanceError: "No se pudo cambiar la apariencia",
    history: "Historial",
    historyHint: "Desplázate hacia arriba para ver traducciones anteriores",
    historyEmpty: "Las traducciones completadas aparecerán aquí",
    historyLoading: "Cargando…",
    historyMore: "Cargar traducciones anteriores",
    historyLoadError: "No se pudo cargar el historial",
    historySaveError: "No se pudo guardar esta traducción en el historial",
    retry: "Reintentar",
    sourceLanguage: "Idioma de origen",
    targetLanguage: "Idioma de destino",
    pasteInput: "Pegar al final",
    clearInput: "Borrar entrada",
    copyOutput: "Copiar resultado",
    clearOutput: "Borrar resultado",
    copied: "Copiado",
    pasteError: "No se puede leer el portapapeles",
    copyError: "Error al copiar",
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
    modelsIdle: "Introduce la URL del servicio para cargar los modelos",
    modelsInvalidUrl: "Introduce una URL HTTP / HTTPS válida",
    modelsLoading: "Comprobando el servicio y cargando modelos…",
    modelsReady: "Servicio disponible · {count} modelos",
    modelsAuth: "Error de autenticación. Comprueba la clave API",
    modelsNetwork: "No se puede conectar. Comprueba la URL o la red",
    modelsTimeout: "Tiempo de conexión agotado. Inténtalo de nuevo",
    modelsHttp: "No se pudieron cargar los modelos (HTTP {status})",
    modelsInvalidResponse: "El servicio no devolvió una lista de modelos válida",
    modelsEmpty: "Servicio conectado, pero no hay modelos disponibles",
    modelConnect: "Conecta para elegir un modelo",
    modelSelect: "Elegir un modelo",
    modelSearch: "Buscar modelos…",
    modelsNoMatch: "No hay modelos coincidentes",
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
    appearanceLabel: "Оформление",
    appearanceDefault: "По умолчанию",
    appearanceGlass: "Жидкое стекло",
    appearanceError: "Не удалось изменить оформление",
    history: "История",
    historyHint: "Прокрутите вверх для просмотра старых переводов",
    historyEmpty: "Завершённые переводы появятся здесь",
    historyLoading: "Загрузка…",
    historyMore: "Загрузить старые переводы",
    historyLoadError: "Не удалось загрузить историю",
    historySaveError: "Не удалось сохранить этот перевод в истории",
    retry: "Повторить",
    sourceLanguage: "Исходный язык",
    targetLanguage: "Язык перевода",
    pasteInput: "Вставить в конец",
    clearInput: "Очистить ввод",
    copyOutput: "Копировать результат",
    clearOutput: "Очистить результат",
    copied: "Скопировано",
    pasteError: "Не удалось прочитать буфер обмена",
    copyError: "Не удалось скопировать",
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
    modelsIdle: "Введите URL сервиса для загрузки моделей",
    modelsInvalidUrl: "Введите корректный HTTP / HTTPS URL",
    modelsLoading: "Проверка сервиса и загрузка моделей…",
    modelsReady: "Сервис доступен · моделей: {count}",
    modelsAuth: "Ошибка авторизации. Проверьте API-ключ",
    modelsNetwork: "Нет подключения. Проверьте URL или сеть",
    modelsTimeout: "Время подключения истекло. Повторите попытку",
    modelsHttp: "Не удалось загрузить модели (HTTP {status})",
    modelsInvalidResponse: "Сервис не вернул корректный список моделей",
    modelsEmpty: "Подключено, но доступных моделей нет",
    modelConnect: "Подключитесь для выбора модели",
    modelSelect: "Выберите модель",
    modelSearch: "Поиск моделей…",
    modelsNoMatch: "Подходящих моделей нет",
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
let languagePickers = [];
let openLanguagePicker = null;
let modelPicker = null;
let modelCatalogState = "idle";
let modelCatalogError = null;
let modelRequestVersion = 0;
let modelFetchTimer = null;
let configuredBaseUrl = "";
let modelDraftUrl = null;
const UI_LANGUAGES = Object.entries(I18N).map(([code, dict]) => ({ value: code, code, label: dict._name }));
const APPEARANCES = [
  { value: "default", labelKey: "appearanceDefault" },
  { value: "liquid_glass", labelKey: "appearanceGlass" },
];

function t(key) {
  return (I18N[currentLang] || I18N.zh)[key];
}

function updateTranslateHint() {
  els.translateHint.textContent = translationInFlight ? t("translating") : translateShortcut;
  els.translateBtn.setAttribute("aria-busy", String(translationInFlight));
}

// 按语言刷新所有界面文案
function applyLocale(lang) {
  currentLang = I18N[lang] ? lang : "zh";
  document.documentElement.lang = currentLang;
  els.swap.title = t("swapTitle");
  els.input.placeholder = t("inputPh");
  els.output.placeholder = t("outputPh");
  els.translateLabel.textContent = t("translate");
  updateTranslateHint();
  updateQuickActions();
  els.openSettings.textContent = t("settings");
  els.closeFlyout.title = t("closeTitle");
  els.closeFlyout.setAttribute("aria-label", t("closeTitle"));
  els.settingsTitle.textContent = t("settingsTitle");
  els.lblUrl.textContent = t("urlLabel");
  els.lblKey.textContent = t("keyLabel");
  els.cfgKeyClear.textContent = t("clearKey");
  els.lblModel.textContent = t("modelLabel");
  els.lblUiLang.textContent = t("uiLangLabel");
  els.lblAppearance.textContent = t("appearanceLabel");
  els.cfgSave.textContent = t("save");
  for (const button of [els.cfgBack, els.historyBack]) {
    button.title = t("back");
    button.setAttribute("aria-label", t("back"));
  }
  els.openHistory.title = t("history");
  els.openHistory.setAttribute("aria-label", t("history"));
  els.historyTitle.textContent = t("history");
  els.historyHint.textContent = t("historyHint");
  els.historyScroll.setAttribute("aria-label", t("history"));
  els.historyRetry.textContent = t("retry");
  renderHistoryEntries();
  updateHistoryState();
  // A(B) 中的 A 随界面语言变化，重建两个语种列表并保留已选值。
  closeLanguagePicker();
  languagePickers.forEach(renderLanguagePicker);
  updateModelCatalogUI();
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

function languageChoiceLabel(language) {
  if (language.labelKey) return t(language.labelKey);
  return language.label ?? langLabel(language.code);
}

function syncLanguagePicker(picker) {
  const selected = picker.items.findIndex((lang) => lang.value === picker.trigger.value);
  picker.label.textContent = selected >= 0 ? languageChoiceLabel(picker.items[selected])
    : picker.allowUnknownValue && picker.trigger.value ? picker.trigger.value
    : t(modelCatalogState === "ready" ? "modelSelect" : "modelConnect");
  picker.label.title = picker.label.textContent;
  picker.trigger.setAttribute("aria-label", `${t(picker.labelKey)}: ${picker.label.textContent}`);
  (picker.list || picker.menu).setAttribute("aria-label", t(picker.labelKey));
  picker.options.forEach((option, index) => {
    option.setAttribute("aria-selected", String(picker.visibleItems[index].value === picker.trigger.value));
  });
}

function renderLanguagePicker(picker) {
  if (!picker.allowUnknownValue && !picker.items.some((lang) => lang.value === picker.trigger.value)) {
    picker.trigger.value = picker.defaultValue;
  }
  const query = picker.searchInput?.value.trim().toLocaleLowerCase(currentLang) || "";
  picker.visibleItems = picker.items.filter((item) => languageChoiceLabel(item).toLocaleLowerCase(currentLang).includes(query));
  picker.options = picker.visibleItems.map((lang, index) => {
    const option = document.createElement("div");
    option.id = `${picker.trigger.id}-option-${picker.searchInput ? index : lang.code || lang.value}`;
    option.className = "language-option";
    option.setAttribute("role", "option");
    const check = document.createElement("span");
    check.className = "language-check";
    check.setAttribute("aria-hidden", "true");
    const label = document.createElement("span");
    label.textContent = languageChoiceLabel(lang);
    label.title = label.textContent;
    option.appendChild(check);
    option.appendChild(label);
    option.addEventListener("pointermove", () => activateLanguageOption(picker, index, false));
    option.addEventListener("click", () => chooseLanguage(picker, index));
    return option;
  });
  (picker.list || picker.menu).replaceChildren(...picker.options);
  if (picker.empty) picker.empty.hidden = picker.options.length > 0;
  syncLanguagePicker(picker);
}

function activateLanguageOption(picker, index, scroll = true) {
  if (!picker.options.length) {
    picker.activeIndex = -1;
    picker.trigger.removeAttribute("aria-activedescendant");
    picker.searchInput?.removeAttribute("aria-activedescendant");
    return;
  }
  index = Math.max(0, Math.min(picker.options.length - 1, index));
  picker.activeIndex = index;
  picker.options.forEach((option, i) => option.classList.toggle("is-active", i === index));
  const option = picker.options[index];
  picker.trigger.setAttribute("aria-activedescendant", option.id);
  picker.searchInput?.setAttribute("aria-activedescendant", option.id);
  if (!scroll) return;
  // Scroll only the list, so keyboard navigation cannot move the flyout itself.
  const top = option.offsetTop;
  const bottom = top + option.offsetHeight;
  const list = picker.list || picker.menu;
  if (top < list.scrollTop) list.scrollTop = top;
  else if (bottom > list.scrollTop + list.clientHeight) {
    list.scrollTop = bottom - list.clientHeight;
  }
}

function closeLanguagePicker(restoreFocus = false) {
  if (!openLanguagePicker) return false;
  const picker = openLanguagePicker;
  openLanguagePicker = null;
  picker.menu.hidden = true;
  picker.trigger.setAttribute("aria-expanded", "false");
  picker.trigger.removeAttribute("aria-activedescendant");
  picker.searchInput?.setAttribute("aria-expanded", "false");
  picker.searchInput?.removeAttribute("aria-activedescendant");
  if (restoreFocus) picker.trigger.focus();
  return true;
}

function showLanguagePicker(picker, index) {
  if (picker.trigger.disabled || !picker.items.length) return;
  closeLanguagePicker();
  openLanguagePicker = picker;
  picker.search = "";
  picker.searchTime = 0;
  if (picker.searchInput) {
    picker.searchInput.value = "";
    renderLanguagePicker(picker);
    picker.searchInput.setAttribute("aria-expanded", "true");
  }
  picker.menu.hidden = false;
  picker.trigger.setAttribute("aria-expanded", "true");
  (picker.searchInput || picker.trigger).focus();
  activateLanguageOption(picker, index ?? picker.visibleItems.findIndex((lang) => lang.value === picker.trigger.value));
}

function chooseLanguage(picker, index) {
  if (picker.trigger.disabled || !picker.visibleItems[index]) return;
  const value = picker.visibleItems[index].value;
  const changed = picker.trigger.value !== value;
  picker.trigger.value = value;
  syncLanguagePicker(picker);
  closeLanguagePicker(true);
  if (changed) return picker.onChange();
}

function onLanguageKeydown(picker, event) {
  if (event.isComposing || event.keyCode === 229 || picker.trigger.disabled) return;
  const expanded = openLanguagePicker === picker;
  if (event.target === picker.searchInput && !["Enter", "ArrowDown", "ArrowUp", "Tab"].includes(event.key)) return;
  if (["Enter", " ", "ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
    event.preventDefault();
    if (!expanded) {
      showLanguagePicker(picker, event.key === "Home" ? 0 : event.key === "End" ? picker.items.length - 1 : undefined);
    } else if (event.key === "Enter" || event.key === " ") {
      return chooseLanguage(picker, picker.activeIndex);
    } else {
      const index = event.key === "Home" ? 0 : event.key === "End" ? picker.visibleItems.length - 1
        : Math.max(0, Math.min(picker.visibleItems.length - 1, picker.activeIndex + (event.key === "ArrowDown" ? 1 : -1)));
      activateLanguageOption(picker, index);
    }
  } else if (event.key === "Tab") {
    closeLanguagePicker();
  } else if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
    event.preventDefault();
    if (!expanded) showLanguagePicker(picker);
    if (picker.searchInput) {
      picker.searchInput.value = event.key;
      renderLanguagePicker(picker);
      activateLanguageOption(picker, 0);
      return;
    }
    const now = Date.now();
    const key = event.key.toLocaleLowerCase(currentLang);
    picker.search = (now - picker.searchTime < 700 ? picker.search : "") + key;
    picker.searchTime = now;
    const find = (query) => {
      for (let step = 1; step <= picker.items.length; step++) {
        const index = (picker.activeIndex + step) % picker.items.length;
        const lang = picker.items[index];
        if ([languageChoiceLabel(lang), ...(lang.code ? [displayName(lang.code, lang.code)] : []), lang.value]
          .some((label) => label.toLocaleLowerCase(currentLang).startsWith(query))) return index;
      }
      return -1;
    };
    let index = find(picker.search);
    if (index < 0) {
      picker.search = key;
      index = find(key);
    }
    if (index >= 0) activateLanguageOption(picker, index);
  }
}

function createLanguagePicker(id, labelKey, defaultValue, items = LANGUAGES, onChange = persistLangs, extra = {}) {
  const picker = {
    root: document.querySelector(`#${id}-picker`),
    trigger: document.querySelector(`#${id}`),
    label: document.querySelector(`#${id}-label`),
    menu: document.querySelector(`#${id}-menu`),
    labelKey, defaultValue, items, onChange, options: [], visibleItems: [], activeIndex: 0, search: "", searchTime: 0,
    ...extra,
  };
  picker.menu.hidden = true;
  picker.trigger.setAttribute("aria-expanded", "false");
  picker.trigger.addEventListener("click", () => {
    if (openLanguagePicker === picker) closeLanguagePicker();
    else showLanguagePicker(picker);
  });
  picker.trigger.addEventListener("keydown", (event) => onLanguageKeydown(picker, event));
  if (picker.searchInput) {
    picker.searchInput.addEventListener("input", () => {
      renderLanguagePicker(picker);
      activateLanguageOption(picker, 0);
    });
    picker.searchInput.addEventListener("keydown", (event) => onLanguageKeydown(picker, event));
  }
  return picker;
}

function normalizeModelUrl(value) {
  try {
    const url = new URL(value.trim());
    if (!["http:", "https:"].includes(url.protocol) || !url.hostname
      || url.username || url.password || url.search || url.hash) return null;
    return url.href.replace(/\/+$/, "");
  } catch {
    return null;
  }
}

function sameConfiguredEndpoint() {
  const url = normalizeModelUrl(els.cfgUrl.value);
  return url !== null && url === normalizeModelUrl(configuredBaseUrl);
}

function updateModelCatalogUI() {
  if (!modelPicker) return;
  const code = modelCatalogError?.code;
  const messageKey = modelCatalogState === "idle" ? "modelsIdle"
    : modelCatalogState === "loading" ? "modelsLoading"
    : modelCatalogState === "ready" ? "modelsReady"
    : code === "invalid_url" ? "modelsInvalidUrl"
    : code === "empty" ? "modelsEmpty"
    : code === "timeout" ? "modelsTimeout"
    : code === "http" && [401, 403].includes(modelCatalogError.status) ? "modelsAuth"
    : code === "http" ? "modelsHttp"
    : code === "invalid_response" ? "modelsInvalidResponse" : "modelsNetwork";
  els.modelStatus.textContent = t(messageKey).replace("{count}", modelPicker.items.length)
    .replace("{status}", modelCatalogError?.status || "");
  els.modelStatus.title = els.modelStatus.textContent;
  els.modelStatus.setAttribute("data-state", modelCatalogState);
  els.modelRetry.textContent = t("retry");
  els.modelRetry.hidden = modelCatalogState !== "error" || code === "invalid_url";
  els.cfgUrl.setAttribute("aria-invalid", String(code === "invalid_url"));
  modelPicker.trigger.disabled = modelCatalogState !== "ready";
  modelPicker.trigger.setAttribute("aria-busy", String(modelCatalogState === "loading"));
  modelPicker.searchInput.placeholder = t("modelSearch");
  modelPicker.searchInput.setAttribute("aria-label", t("modelSearch"));
  modelPicker.empty.textContent = t("modelsNoMatch");
  syncLanguagePicker(modelPicker);
}

async function fetchModelCatalog(version) {
  try {
    const models = await invoke("list_models", {
      baseUrl: els.cfgUrl.value.trim(),
      apiKey: apiKeyChanged ? els.cfgKey.value.trim() : null,
    });
    if (version !== modelRequestVersion) return;
    if (!Array.isArray(models) || models.some((id) => typeof id !== "string" || !id.trim())) {
      throw { code: "invalid_response" };
    }
    if (!models.length) throw { code: "empty" };
    modelPicker.items = [...new Set(models)].map((value) => ({ value, label: value }));
    if (!modelPicker.items.some((item) => item.value === modelPicker.trigger.value)) {
      modelPicker.trigger.value = "";
    }
    modelCatalogState = "ready";
    modelCatalogError = null;
  } catch (error) {
    if (version !== modelRequestVersion) return;
    modelCatalogState = "error";
    modelCatalogError = error;
    modelPicker.items = [];
  }
  renderLanguagePicker(modelPicker);
  updateModelCatalogUI();
}

function refreshModelCatalog(delay = 450) {
  const version = ++modelRequestVersion;
  clearTimeout(modelFetchTimer);
  modelFetchTimer = null;
  if (openLanguagePicker === modelPicker) closeLanguagePicker();
  modelPicker.items = [];
  renderLanguagePicker(modelPicker);
  modelCatalogError = null;
  const rawUrl = els.cfgUrl.value.trim();
  if (!rawUrl) modelCatalogState = "idle";
  else if (!normalizeModelUrl(rawUrl)) {
    modelCatalogState = "error";
    modelCatalogError = { code: "invalid_url" };
  } else modelCatalogState = "loading";
  updateModelCatalogUI();
  if (modelCatalogState !== "loading") return;
  if (!delay) return fetchModelCatalog(version);
  modelFetchTimer = setTimeout(() => {
    modelFetchTimer = null;
    return fetchModelCatalog(version);
  }, delay);
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
let pasteInFlight = false;
let copyInFlight = false;
let inputClearVersion = 0;
let resultRevision = 0;
let copied = false;
let copyFeedbackTimer = null;
let clipboardStatus = "";
let currentAppearance = "default";
let appearanceRevision = 0;
let appearanceStatus = "";
let windowAppearanceRevision = 0;
let nativeReveal = false;
let glassLayoutFrame = null;
let glassLayoutRevision = 0;
let glassLayoutSignature = "";
let historyEntries = [];
let historyPendingEntries = [];
let historyHasMore = false;
let historyLoaded = false;
let historyLoading = false;
let historyLoadError = "";
let historySaveError = "";
let historyRequestVersion = 0;
let historyUpdateVersion = 0;

function historyWarning() {
  return historySaveError ? `${t("historySaveError")}: ${historySaveError}` : "";
}

function updateHistoryState() {
  els.historyScroll.setAttribute("aria-busy", String(historyLoading));
  els.historyMore.classList.toggle("hidden", !historyHasMore);
  els.historyMore.disabled = historyLoading;
  els.historyMore.textContent = t(historyLoading ? "historyLoading" : "historyMore");
  els.historyEmpty.classList.toggle("hidden", historyEntries.length > 0 || Boolean(historyLoadError));
  els.historyEmpty.textContent = t(historyLoading ? "historyLoading" : "historyEmpty");
  const message = historyLoadError ? `${t("historyLoadError")}: ${historyLoadError}` : historyWarning();
  els.historyStatus.textContent = message;
  els.historyNotice.classList.toggle("hidden", !message);
  els.historyRetry.classList.toggle("hidden", !historyLoadError);
  els.historyRetry.disabled = historyLoading;
}

function historyLanguage(value) {
  const language = LANGUAGES.find((lang) => lang.value === value);
  return language ? displayName(currentLang, language.code) : value;
}

function renderHistoryEntries() {
  const formatter = new Intl.DateTimeFormat(currentLang, {
    year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit",
  });
  els.historyList.replaceChildren(...historyEntries.map((entry) => {
    const item = document.createElement("article");
    item.className = "history-entry";
    item.setAttribute("role", "listitem");
    item.setAttribute("data-history-id", String(entry.id));
    const meta = document.createElement("div");
    meta.className = "history-meta";
    const languages = document.createElement("span");
    languages.textContent = `${historyLanguage(entry.lang_a)} → ${historyLanguage(entry.lang_b)}`;
    const time = document.createElement("time");
    const date = new Date(entry.created_at);
    time.dateTime = date.toISOString();
    time.textContent = formatter.format(date);
    meta.appendChild(languages);
    meta.appendChild(time);
    const texts = document.createElement("div");
    texts.className = "history-texts";
    for (const [text, language] of [[entry.source_text, entry.lang_a], [entry.translation, entry.lang_b]]) {
      const paragraph = document.createElement("p");
      paragraph.textContent = text;
      paragraph.dir = "auto";
      paragraph.lang = LANGUAGES.find((lang) => lang.value === language)?.code || "";
      texts.appendChild(paragraph);
    }
    item.appendChild(meta);
    item.appendChild(texts);
    return item;
  }));
}

function mergeHistory(...groups) {
  return [...new Map(groups.flat().map((entry) => [entry.id, entry])).values()]
    .sort((a, b) => a.id - b.id);
}

function historyAtBottom() {
  return els.historyScroll.scrollHeight - els.historyScroll.clientHeight - els.historyScroll.scrollTop < 40;
}

async function loadHistory(reset = false) {
  if (historyLoading && !reset) return;
  if (!reset && !historyHasMore) return;
  const version = ++historyRequestVersion;
  if (reset) {
    historyEntries = [];
    historyPendingEntries = [];
    historyHasMore = false;
    historyLoaded = false;
    renderHistoryEntries();
  }
  historyLoading = true;
  historyLoadError = "";
  const updateVersion = historyUpdateVersion;
  updateHistoryState();
  try {
    const page = await invoke("load_history", { before: reset ? null : historyEntries[0]?.id });
    if (version !== historyRequestVersion || currentPage !== "history") return;
    const previousHeight = els.historyScroll.scrollHeight;
    const previousTop = els.historyScroll.scrollTop;
    historyEntries = mergeHistory(historyEntries, page.entries, historyPendingEntries);
    historyPendingEntries = [];
    historyHasMore = page.has_more;
    if (updateVersion === historyUpdateVersion) historySaveError = page.save_error || "";
    historyLoaded = true;
    renderHistoryEntries();
    historyLoading = false;
    updateHistoryState();
    // Prepending older entries must keep the text being read in the same place.
    els.historyScroll.scrollTop = reset ? els.historyScroll.scrollHeight
      : previousTop + els.historyScroll.scrollHeight - previousHeight;
  } catch (error) {
    if (version === historyRequestVersion) historyLoadError = String(error);
  } finally {
    if (version === historyRequestVersion) {
      historyLoading = false;
      updateHistoryState();
    }
  }
}

function onHistoryUpdated({ entry, error }) {
  historyUpdateVersion++;
  const previousWarning = historyWarning();
  historySaveError = error || "";
  if (currentPage === "translate" && (historySaveError || els.status.textContent === previousWarning)) {
    els.status.textContent = historyWarning();
  }
  if (entry && currentPage === "history") {
    if (!historyLoaded) historyPendingEntries.push(entry);
    else {
      const atBottom = historyAtBottom();
      historyEntries = mergeHistory(historyEntries, [entry]);
      renderHistoryEntries();
      updateHistoryState();
      if (atBottom) els.historyScroll.scrollTop = els.historyScroll.scrollHeight;
    }
  }
  updateHistoryState();
}

function clearClipboardStatus() {
  if (clipboardStatus && els.status.textContent === clipboardStatus) els.status.textContent = "";
  clipboardStatus = "";
}

function showClipboardError(key, error) {
  clipboardStatus = `${t(key)}: ${error}`;
  els.status.textContent = clipboardStatus;
}

function updateQuickActions() {
  els.pasteInput.disabled = pasteInFlight;
  els.clearInput.disabled = !els.input.value;
  els.copyOutput.disabled = !els.output.value || copyInFlight;
  els.clearOutput.disabled = !els.output.value;
  for (const [button, label] of [
    [els.pasteInput, "pasteInput"], [els.clearInput, "clearInput"],
    [els.copyOutput, copied ? "copied" : "copyOutput"], [els.clearOutput, "clearOutput"],
  ]) {
    button.title = t(label);
    button.setAttribute("aria-label", t(label));
  }
}

function resetCopyFeedback() {
  clearTimeout(copyFeedbackTimer);
  copyFeedbackTimer = null;
  copied = false;
  els.copyOutput.classList.remove("copied");
}

async function pasteInput() {
  if (pasteInFlight) return;
  clearClipboardStatus();
  const version = inputClearVersion;
  pasteInFlight = true;
  updateQuickActions();
  try {
    const text = await invoke("plugin:clipboard-manager|read_text");
    if (version !== inputClearVersion || typeof text !== "string" || !text) return;
    els.input.value += text;
    els.input.setSelectionRange(els.input.value.length, els.input.value.length);
    els.input.scrollTop = els.input.scrollHeight;
    if (currentPage === "translate" && !closeRequested) els.input.focus();
  } catch (error) {
    const message = error?.message ?? String(error);
    if (version === inputClearVersion && message !== NON_TEXT_CLIPBOARD) showClipboardError("pasteError", error);
  } finally {
    pasteInFlight = false;
    updateQuickActions();
  }
}

async function copyOutput() {
  const text = els.output.value;
  if (!text || copyInFlight) return;
  clearClipboardStatus();
  const revision = resultRevision;
  resetCopyFeedback();
  copyInFlight = true;
  updateQuickActions();
  try {
    await invoke("plugin:clipboard-manager|write_text", { text });
    if (revision !== resultRevision || els.output.value !== text) return;
    resetCopyFeedback();
    copied = true;
    els.copyOutput.classList.add("copied");
    copyFeedbackTimer = setTimeout(() => {
      resetCopyFeedback();
      updateQuickActions();
    }, 1500);
  } catch (error) {
    if (revision === resultRevision && els.output.value === text) {
      showClipboardError("copyError", error);
    }
  } finally {
    copyInFlight = false;
    updateQuickActions();
  }
}

function clearInput() {
  inputClearVersion++;
  resultRevision++;
  els.input.value = "";
  els.input.focus();
  updateQuickActions();
}

function clearOutput() {
  resultRevision++;
  els.output.value = "";
  resetCopyFeedback();
  els.output.focus();
  updateQuickActions();
}

function updateApiKeyUI() {
  const savedKeyApplies = apiKeyConfigured && sameConfiguredEndpoint();
  els.cfgKey.placeholder = !apiKeyChanged && savedKeyApplies ? "••••••••" : "sk-…";
  const hasKey = apiKeyChanged ? Boolean(els.cfgKey.value.trim()) : savedKeyApplies;
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
  const generation = flyoutGeneration;
  // 双 rAF：先让“隐藏态”绘制一帧，再触发过渡，避免直接闪现
  firstOpenFrame = requestAnimationFrame(() => {
    firstOpenFrame = null;
    secondOpenFrame = requestAnimationFrame(() => {
      secondOpenFrame = null;
      els.card.classList.add("show");
      const layout = captureGlassSurfaces();
      if (nativeReveal) {
        invoke("reveal_flyout", { generation, ...layout }).catch((error) => {
          if (generation === flyoutGeneration && !closeRequested) {
            appearanceStatus = `${t("appearanceError")}: ${error}`;
            showConfigStatus(appearanceStatus);
          }
        });
      }
    });
  });
}

// 卡片向下滑出（收起），动画结束后再真正隐藏窗口——这样能看到下滑过程
function slideOutThenHide(generation) {
  if (generation !== flyoutGeneration || hideTimer !== null) return;
  closeLanguagePicker();
  closeRequested = true;
  cancelPageFocus();
  cancelSlideInFrames();
  if (glassLayoutFrame !== null) cancelAnimationFrame(glassLayoutFrame);
  glassLayoutFrame = null;
  els.card.classList.remove("show");
  hideTimer = setTimeout(() => {
    hideTimer = null;
    invoke("commit_hide", { generation }).catch((e) => {
      if (generation === flyoutGeneration) els.status.textContent = String(e);
    });
  }, CLOSE_MS + 40);
}

function showPage(page) {
  closeLanguagePicker();
  currentPage = ["settings", "history"].includes(page) ? page : "translate";
  els.pageTranslate.classList.toggle("hidden", currentPage !== "translate");
  els.pageSettings.classList.toggle("hidden", currentPage !== "settings");
  els.pageHistory.classList.toggle("hidden", currentPage !== "history");
  scheduleGlassSurfaces();
  // Invalidate an older history request when leaving or reopening the page.
  historyRequestVersion++;
  historyLoading = false;
  // 显示后立刻聚焦输入框，让中文输入法候选框贴着光标出现（修复其跑到左上角的问题）
  cancelPageFocus();
  focusFrame = requestAnimationFrame(() => {
    focusFrame = null;
    const target = currentPage === "settings" ? els.cfgUrl
      : currentPage === "history" ? els.historyScroll : els.input;
    target.focus({ preventScroll: true });
  });
  if (currentPage === "history") return loadHistory(true);
}

async function doTranslate() {
  if (translationInFlight) return;
  const text = els.input.value;
  if (!text.trim()) {
    els.output.value = "";
    resetCopyFeedback();
    updateQuickActions();
    return;
  }
  const langA = els.langA.value;
  const langB = els.langB.value;
  const revision = resultRevision;
  const isCurrentInput = () =>
    resultRevision === revision && els.input.value === text && els.langA.value === langA && els.langB.value === langB;
  translationInFlight = true;
  els.status.textContent = "";
  els.translateBtn.disabled = true;
  updateTranslateHint();
  try {
    const result = await invoke("translate", {
      text,
      langA,
      langB,
    });
    if (!isCurrentInput()) return;
    els.output.value = result;
    resetCopyFeedback();
    els.status.textContent = historyWarning();
  } catch (e) {
    if (!isCurrentInput()) return;
    els.output.value = "";
    resetCopyFeedback();
    els.status.textContent = String(e);
  } finally {
    translationInFlight = false;
    els.translateBtn.disabled = false;
    updateTranslateHint();
    updateQuickActions();
  }
}

async function loadConfigIntoUI() {
  const cfg = await invoke("load_config");
  els.cfgUrl.value = cfg.base_url || "";
  configuredBaseUrl = els.cfgUrl.value;
  modelDraftUrl = normalizeModelUrl(els.cfgUrl.value);
  apiKeyConfigured = Boolean(cfg.api_key_configured);
  apiKeyChanged = false;
  els.cfgKey.value = "";
  updateApiKeyUI();
  els.cfgModel.value = cfg.model || "";
  els.langA.value = cfg.lang_a || "Chinese";
  els.langB.value = cfg.lang_b || "English";
  els.cfgUiLang.value = cfg.ui_lang || "zh";
  applyAppearance(cfg.appearance);
  applyLocale(els.cfgUiLang.value);
  configLoadError = cfg.load_error || "";
  showConfigStatus(configLoadError);
  els.status.textContent = configLoadError;
  // Discovery must not delay the frontend-ready handshake or opening the flyout.
  void refreshModelCatalog(0);
}

async function saveConfig() {
  if (configSaveInFlight) return;
  if (!normalizeModelUrl(els.cfgUrl.value)) {
    showConfigStatus(t("modelsInvalidUrl"));
    return;
  }
  if (!els.cfgModel.value.trim()) {
    showConfigStatus(t("modelSelect"));
    return;
  }
  const keyRevision = apiKeyRevision;
  const config = {
    base_url: els.cfgUrl.value.trim(),
    api_key: apiKeyChanged ? els.cfgKey.value.trim() : sameConfiguredEndpoint() ? null : "",
    model: els.cfgModel.value.trim(),
    lang_a: els.langA.value,
    lang_b: els.langB.value,
    ui_lang: els.cfgUiLang.value,
    appearance: els.cfgAppearance.value,
  };
  configSaveInFlight = true;
  els.cfgSave.disabled = true;
  try {
    await queueConfigWrite("save_config", { config });
    configuredBaseUrl = config.base_url;
    const manualDraftUnchanged = apiKeyRevision === keyRevision
      && els.cfgUrl.value.trim() === config.base_url
      && els.cfgModel.value.trim() === config.model
      && els.cfgAppearance.value === config.appearance;
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

async function changeUiLanguage() {
  applyLocale(els.cfgUiLang.value);
  await persistUiLang();
}

function applyAppearance(value) {
  currentAppearance = APPEARANCES.some((option) => option.value === value) ? value : "default";
  els.cfgAppearance.value = currentAppearance;
  document.documentElement.setAttribute("data-appearance", currentAppearance);
  if (currentAppearance !== "liquid_glass") {
    els.card.style.removeProperty("--glass-background-mask");
    glassLayoutSignature = "";
  }
  const picker = languagePickers.find((item) => item.trigger === els.cfgAppearance);
  if (picker) syncLanguagePicker(picker);
}

function applyNativeReveal(enabled) {
  // Always release a mask the backend prepared, even if a simultaneous preference
  // change made the opaque renderer current before this navigation arrived.
  nativeReveal = enabled === true;
  document.documentElement.setAttribute("data-native-reveal", String(nativeReveal));
}

// Only geometry crosses the bridge. The same rounded regions cut holes in the
// HTML background tint and retain full native frost underneath the controls.
function captureGlassSurfaces() {
  const bounds = els.card.getBoundingClientRect();
  const round = (value) => Math.round(value * 100) / 100;
  const surfaces = [];
  if (currentAppearance === "liquid_glass") {
    const selectors = ".pane, .lang-select, .swap, .secondary, .translate-btn, #open-history, .page-back, .history-entry, #page-settings input, .language-menu";
    for (const element of document.querySelectorAll(selectors)) {
      const rect = element.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) continue;
      let left = Math.max(bounds.left, rect.left);
      let top = Math.max(bounds.top, rect.top);
      let right = Math.min(bounds.right, rect.right);
      let bottom = Math.min(bounds.bottom, rect.bottom);
      for (let parent = element.parentElement; parent && parent !== els.card; parent = parent.parentElement) {
        const style = getComputedStyle(parent);
        if (/(auto|scroll|hidden|clip)/.test(`${style.overflowX} ${style.overflowY}`)) {
          const clip = parent.getBoundingClientRect();
          left = Math.max(left, clip.left);
          top = Math.max(top, clip.top);
          right = Math.min(right, clip.right);
          bottom = Math.min(bottom, clip.bottom);
        }
      }
      if (right <= left || bottom <= top) continue;
      const radius = Math.min(parseFloat(getComputedStyle(element).borderTopLeftRadius) || 0, rect.width / 2, rect.height / 2);
      surfaces.push({
        rect: { x: round(rect.left - bounds.left), y: round(rect.top - bounds.top), width: round(rect.width), height: round(rect.height) },
        clip: { x: round(left - bounds.left), y: round(top - bounds.top), width: round(right - left), height: round(bottom - top) },
        radius: round(radius),
      });
      if (surfaces.length === 128) break;
    }
    const signature = JSON.stringify([bounds.width, bounds.height, surfaces]);
    if (signature !== glassLayoutSignature) {
      glassLayoutSignature = signature;
      const rectangle = ({ x, y, width, height }) => `x="${x}" y="${y}" width="${width}" height="${height}"`;
      const clips = surfaces.map((surface, index) => `<clipPath id="c${index}"><rect ${rectangle(surface.clip)}/></clipPath>`).join("");
      const holes = surfaces.map((surface, index) => `<rect ${rectangle(surface.rect)} rx="${surface.radius}" fill="black" clip-path="url(#c${index})"/>`).join("");
      const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${bounds.width}" height="${bounds.height}" viewBox="0 0 ${bounds.width} ${bounds.height}"><defs>${clips}<mask id="background" maskUnits="userSpaceOnUse" x="0" y="0" width="${bounds.width}" height="${bounds.height}"><rect width="100%" height="100%" fill="white"/>${holes}</mask></defs><rect width="100%" height="100%" fill="white" mask="url(#background)"/></svg>`;
      els.card.style.setProperty("--glass-background-mask", `url("data:image/svg+xml,${encodeURIComponent(svg)}")`);
    }
  }
  return { surfaces, layoutRevision: ++glassLayoutRevision };
}

function scheduleGlassSurfaces() {
  if (glassLayoutFrame !== null || currentAppearance !== "liquid_glass") return;
  glassLayoutFrame = requestAnimationFrame(() => {
    glassLayoutFrame = null;
    if (closeRequested || !els.card.classList.contains("show") || currentAppearance !== "liquid_glass") return;
    const previous = glassLayoutSignature;
    const layout = captureGlassSurfaces();
    if (!nativeReveal || previous === glassLayoutSignature) return;
    const generation = flyoutGeneration;
    invoke("update_glass_surfaces", { generation, revision: layout.layoutRevision, surfaces: layout.surfaces }).catch((error) => {
      if (generation === flyoutGeneration && !closeRequested && layout.layoutRevision === glassLayoutRevision) {
        appearanceStatus = `${t("appearanceError")}: ${error}`;
        showConfigStatus(appearanceStatus);
      }
    });
  });
}

async function syncWindowAppearance(revision = appearanceRevision) {
  if (!els.card.classList.contains("show") || closeRequested) return;
  const generation = flyoutGeneration;
  const nativeRevision = ++windowAppearanceRevision;
  try {
    // Only explicit appearance changes update material. Opening already prepared
    // it before the native window became visible; transitionend does no setup.
    const native = await invoke("set_window_appearance", {
      appearance: currentAppearance, generation, revision: nativeRevision, ...captureGlassSurfaces(),
    });
    if (revision === appearanceRevision && generation === flyoutGeneration
      && nativeRevision === windowAppearanceRevision && !closeRequested) {
      applyNativeReveal(native);
      if (native) {
        // Layout can change while the material request is pending (for example,
        // a settings label wraps). Resend current regions after enabling it.
        glassLayoutSignature = "";
        scheduleGlassSurfaces();
      }
    }
  } catch (error) {
    if (revision === appearanceRevision && generation === flyoutGeneration
      && nativeRevision === windowAppearanceRevision && !closeRequested) {
      appearanceStatus = `${t("appearanceError")}: ${error}`;
      showConfigStatus(appearanceStatus);
    }
  }
}

async function changeAppearance() {
  const revision = ++appearanceRevision;
  applyAppearance(els.cfgAppearance.value);
  if (appearanceStatus && els.cfgStatus.textContent === appearanceStatus) showConfigStatus("");
  appearanceStatus = "";
  const nativeUpdate = syncWindowAppearance(revision);
  try {
    await queueConfigWrite("save_appearance", { appearance: currentAppearance });
  } catch (error) {
    if (revision === appearanceRevision) {
      appearanceStatus = `${t("appearanceError")}: ${error}`;
      showConfigStatus(appearanceStatus);
    }
  }
  await nativeUpdate;
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
    pageHistory: document.querySelector("#page-history"),
    openHistory: document.querySelector("#open-history"),
    historyBack: document.querySelector("#history-back"),
    historyTitle: document.querySelector("#history-title"),
    historyHint: document.querySelector("#history-hint"),
    historyScroll: document.querySelector("#history-scroll"),
    historyList: document.querySelector("#history-list"),
    historyEmpty: document.querySelector("#history-empty"),
    historyNotice: document.querySelector("#history-notice"),
    historyStatus: document.querySelector("#history-status"),
    historyRetry: document.querySelector("#history-retry"),
    historyMore: document.querySelector("#history-more"),
    langA: document.querySelector("#lang-a"),
    langB: document.querySelector("#lang-b"),
    swap: document.querySelector("#swap"),
    input: document.querySelector("#input"),
    output: document.querySelector("#output"),
    pasteInput: document.querySelector("#paste-input"),
    clearInput: document.querySelector("#clear-input"),
    copyOutput: document.querySelector("#copy-output"),
    clearOutput: document.querySelector("#clear-output"),
    translateBtn: document.querySelector("#translate-btn"),
    translateLabel: document.querySelector("#translate-label"),
    translateHint: document.querySelector("#translate-hint"),
    status: document.querySelector("#status"),
    cfgUrl: document.querySelector("#cfg-url"),
    cfgKey: document.querySelector("#cfg-key"),
    cfgKeyClear: document.querySelector("#cfg-key-clear"),
    cfgModel: document.querySelector("#cfg-model"),
    modelStatus: document.querySelector("#cfg-model-status"),
    modelRetry: document.querySelector("#cfg-model-retry"),
    cfgUiLang: document.querySelector("#cfg-ui-lang"),
    cfgAppearance: document.querySelector("#cfg-appearance"),
    openSettings: document.querySelector("#open-settings"),
    cfgSave: document.querySelector("#cfg-save"),
    cfgBack: document.querySelector("#cfg-back"),
    cfgStatus: document.querySelector("#cfg-status"),
    settingsTitle: document.querySelector("#settings-title"),
    lblUrl: document.querySelector("#lbl-url"),
    lblKey: document.querySelector("#lbl-key"),
    lblModel: document.querySelector("#lbl-model"),
    lblUiLang: document.querySelector("#lbl-uilang"),
    lblAppearance: document.querySelector("#lbl-appearance"),
  };

  els.translateBtn.setAttribute("aria-keyshortcuts", isMac ? "Meta+Enter" : "Control+Enter");
  // AppKit supplies desktop blur. A second WebKit backdrop filter changes its
  // compositing when a clip animation or language dropdown becomes a layer.
  document.documentElement.setAttribute("data-native-backdrop", String(isMac));
  modelPicker = createLanguagePicker("cfg-model", "modelLabel", "", [], () => {
    if (!configLoadError) showConfigStatus("");
  }, {
    allowUnknownValue: true,
    searchInput: document.querySelector("#cfg-model-search"),
    list: document.querySelector("#cfg-model-options"),
    empty: document.querySelector("#cfg-model-empty"),
  });
  languagePickers = [
    createLanguagePicker("lang-a", "sourceLanguage", "Chinese"),
    createLanguagePicker("lang-b", "targetLanguage", "English"),
    createLanguagePicker("cfg-ui-lang", "uiLangLabel", "zh", UI_LANGUAGES, changeUiLanguage),
    createLanguagePicker("cfg-appearance", "appearanceLabel", "default", APPEARANCES, changeAppearance),
    modelPicker,
  ];
  els.langA.value = "Chinese";
  els.langB.value = "English";
  els.cfgUiLang.value = "zh";
  applyAppearance("default");
  applyLocale("zh");
  updateApiKeyUI();

  await Promise.all([
    listen("navigate", (e) => {
      const payload = e.payload;
      const page = typeof payload === "string" ? payload : payload?.page;
      const origin = typeof payload === "string" ? "bottom" : payload?.origin;
      flyoutGeneration = payload?.generation ?? flyoutGeneration;
      applyNativeReveal(payload?.nativeReveal);
      if (payload?.appearanceError) {
        appearanceStatus = `${t("appearanceError")}: ${payload.appearanceError}`;
        showConfigStatus(appearanceStatus);
      }
      showPage(page || "translate");
      slideIn(origin || "bottom");
    }),
    listen("flyout-hide", (e) => slideOutThenHide(e.payload?.generation)),
    listen("history-updated", (e) => onHistoryUpdated(e.payload)),
  ]);

  els.translateBtn.addEventListener("click", doTranslate);
  els.pasteInput.addEventListener("click", pasteInput);
  els.clearInput.addEventListener("click", clearInput);
  els.copyOutput.addEventListener("click", copyOutput);
  els.clearOutput.addEventListener("click", clearOutput);
  els.input.addEventListener("input", updateQuickActions);
  els.closeFlyout.addEventListener("click", requestClose);
  els.cfgUrl.addEventListener("input", () => {
    const nextUrl = normalizeModelUrl(els.cfgUrl.value);
    if (nextUrl !== modelDraftUrl) els.cfgModel.value = "";
    modelDraftUrl = nextUrl;
    updateApiKeyUI();
    refreshModelCatalog();
  });
  for (const field of [els.cfgUrl, els.cfgKey]) {
    field.addEventListener("change", () => {
      if (modelFetchTimer !== null) return refreshModelCatalog(0);
    });
  }
  els.modelRetry.addEventListener("click", () => refreshModelCatalog(0));
  els.cfgKey.addEventListener("input", () => {
    apiKeyChanged = true;
    apiKeyRevision++;
    updateApiKeyUI();
    refreshModelCatalog();
  });
  els.cfgKeyClear.addEventListener("click", () => {
    apiKeyChanged = true;
    apiKeyRevision++;
    els.cfgKey.value = "";
    updateApiKeyUI();
    refreshModelCatalog(0);
  });
  // Command/Ctrl+Enter 快速翻译，输入法确认不触发提交。
  els.input.addEventListener("keydown", (e) => {
    if (e.isComposing || e.keyCode === 229) return;
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      doTranslate();
    }
  });

  els.swap.addEventListener("click", async () => {
    closeLanguagePicker();
    const a = els.langA.value;
    els.langA.value = els.langB.value;
    els.langB.value = a;
    languagePickers.forEach(syncLanguagePicker);
    // 同时交换输入/输出，方便反向确认
    const t = els.input.value;
    els.input.value = els.output.value;
    els.output.value = t;
    resetCopyFeedback();
    updateQuickActions();
    await persistLangs();
  });

  els.openSettings.addEventListener("click", () => showPage("settings"));
  els.openHistory.addEventListener("click", () => showPage("history"));
  els.historyBack.addEventListener("click", () => showPage("translate"));
  els.historyMore.addEventListener("click", () => loadHistory());
  els.historyRetry.addEventListener("click", () => loadHistory(!historyLoaded));
  els.historyScroll.addEventListener("scroll", () => {
    if (currentPage === "history" && els.historyScroll.scrollTop < 48 && historyHasMore) loadHistory();
  });
  els.cfgSave.addEventListener("click", saveConfig);
  els.cfgBack.addEventListener("click", () => showPage("translate"));

  // 切换界面语言：立即刷新文案并持久化
  els.cfgUiLang.addEventListener("change", changeUiLanguage);
  els.cfgAppearance.addEventListener("change", changeAppearance);
  // Scroll and content changes can expose different rounded surface regions.
  document.addEventListener("scroll", scheduleGlassSurfaces, true);
  window.addEventListener("resize", scheduleGlassSurfaces);
  new MutationObserver(scheduleGlassSurfaces).observe(els.card, {
    childList: true, subtree: true, attributes: true, attributeFilter: ["class", "hidden"],
  });
  if (typeof ResizeObserver !== "undefined") {
    const observer = new ResizeObserver(scheduleGlassSurfaces);
    [els.card, els.pageTranslate, els.pageSettings, els.pageHistory].forEach((element) => observer.observe(element));
  }

  // Close an open list before handling the flyout's Escape shortcut.
  document.addEventListener("pointerdown", (event) => {
    if (openLanguagePicker && !openLanguagePicker.root.contains(event.target)) closeLanguagePicker();
  });
  document.addEventListener("focusin", (event) => {
    if (openLanguagePicker && !openLanguagePicker.root.contains(event.target)) closeLanguagePicker();
  });
  window.addEventListener("blur", () => closeLanguagePicker());
  document.addEventListener("keydown", (e) => {
    if (e.defaultPrevented || e.isComposing || e.keyCode === 229) return;
    if (e.key === "Escape") {
      if (closeLanguagePicker(true)) e.preventDefault();
      else requestClose();
    }
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
