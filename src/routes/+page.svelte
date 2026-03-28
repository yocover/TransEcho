<script lang="ts">
  import { invoke, Channel } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { listen } from "@tauri-apps/api/event";
  import { detectLocale, t, translateStatus, type Locale } from "$lib/i18n";

  let appVersion = $state("");
  const uiLang: Locale = detectLocale();

  function getSystemLang(): string {
    const lang = navigator.language.toLowerCase().split("-")[0];
    const supported = ["zh", "en", "ja", "de", "fr", "es", "pt", "id"];
    return supported.includes(lang) ? lang : "zh";
  }

  let apiKey = $state("");
  let sourceLang = $state("ja");
  let targetLang = $state(getSystemLang());
  let enableTts = $state(true);
  let speakerId = $state("zh_female_xiaoai_uranus_bigtts");
  let cloneVoice = $state(false);
  let hotWords = $state("");
  let glossary = $state("");
  let correctWords = $state("");
  let isRunning = $state(false);
  let status = $state("");
  let subtitles: Array<{ source: string; translation: string }> = $state([]);
  let currentSource = $state("");
  let currentTranslation = $state("");
  let subtitleEl: HTMLElement;
  let wasAtBottom = $state(true);

  async function loadSettings() {
    try {
      appVersion = await getVersion();
    } catch (_) {}
    try {
      const { load } = await import("@tauri-apps/plugin-store");
      const store = await load("settings.json");
      apiKey = (await store.get<string>("api_key")) || "";
      const sl = await store.get<string>("source_lang");
      const tl = await store.get<string>("target_lang");
      if (sl) sourceLang = sl;
      if (tl) targetLang = tl;
      const ttsMode = (await store.get<string>("tts_mode")) || "preset";
      enableTts = ttsMode !== "off";
      cloneVoice = ttsMode === "clone";
      speakerId = (await store.get<string>("speaker_id")) || "zh_female_xiaoai_uranus_bigtts";
      hotWords = (await store.get<string>("hot_words")) || "";
      glossary = (await store.get<string>("glossary")) || "";
      correctWords = (await store.get<string>("correct_words")) || "";
      if (!apiKey) openSettings();
    } catch (e) {
      openSettings();
    }
  }

  async function openSettings() {
    const existing = await WebviewWindow.getByLabel("settings");
    if (existing) {
      await existing.setFocus();
      return;
    }
    new WebviewWindow("settings", {
      url: "/settings",
      title: t(uiLang, "settings"),
      width: 550,
      height: 750,
      center: true,
      resizable: true,
      minimizable: false,
      maximizable: false,
    });
  }

  async function saveLanguages() {
    try {
      const { load } = await import("@tauri-apps/plugin-store");
      const store = await load("settings.json");
      await store.set("source_lang", sourceLang);
      await store.set("target_lang", targetLang);
      await store.save();
    } catch (_) {}
  }

  async function start() {
    const curSource = sourceLang;
    const curTarget = targetLang;
    await loadSettings();
    sourceLang = curSource;
    targetLang = curTarget;
    if (!apiKey) {
      openSettings();
      return;
    }

    await saveLanguages();

    const onSubtitle = new Channel<{
      type: string;
      text?: string;
      is_final?: boolean;
      spk_chg?: boolean;
      message?: string;
      input_audio_tokens?: number;
      output_text_tokens?: number;
      output_audio_tokens?: number;
      duration_ms?: number;
    }>();

    onSubtitle.onmessage = (event) => {
      switch (event.type) {
        case "source":
          if (event.text) currentSource = event.text;
          break;
        case "translation":
          if (event.is_final && event.text) {
            currentTranslation = event.text;
            if (currentSource || currentTranslation) {
              // Deduplicate: skip if translation text matches any of the last 10 entries
              const recent = subtitles.slice(-10);
              const isDup = recent.some(
                (s) => s.translation === currentTranslation
              );
              if (!isDup) {
                subtitles = [
                  ...subtitles.slice(-19),
                  { source: currentSource, translation: currentTranslation },
                ];
              }
              currentSource = "";
              currentTranslation = "";
            }
          } else if (event.text) {
            currentTranslation = event.text;
          }
          break;
        case "status":
          status = event.message ? translateStatus(uiLang, event.message) : "";
          // Handle terminal states from backend
          if (event.message === "stopped" || event.message === "session_ended") {
            isRunning = false;
          }
          break;
        case "error":
          status = event.message ? translateStatus(uiLang, event.message) : t(uiLang, "unknown_error");
          isRunning = false;
          break;
      }
    };

    try {
      isRunning = true;
      status = "";
      // Parse corpus fields
      const hotWordsList = hotWords.split("\n").map(s => s.trim()).filter(Boolean);
      const glossaryMap: Record<string, string> = {};
      for (const line of glossary.split("\n")) {
        const eq = line.indexOf("=");
        if (eq > 0) glossaryMap[line.slice(0, eq).trim()] = line.slice(eq + 1).trim();
      }
      const correctWordsMap: Record<string, string> = {};
      for (const line of correctWords.split("\n")) {
        const eq = line.indexOf("=");
        if (eq > 0) correctWordsMap[line.slice(0, eq).trim()] = line.slice(eq + 1).trim();
      }
      const correctWordsJson = Object.keys(correctWordsMap).length > 0 ? JSON.stringify(correctWordsMap) : "";

      await invoke("start_interpretation", {
        onSubtitle,
        apiKey,
        sourceLanguage: sourceLang,
        targetLanguage: targetLang,
        enableTts: enableTts,
        speakerId: enableTts && !cloneVoice ? speakerId : "",
        hotWords: hotWordsList,
        glossary: glossaryMap,
        correctWords: correctWordsJson,
      });
    } catch (e) {
      status = translateStatus(uiLang, `${e}`);
      isRunning = false;
    }
  }

  async function stop() {
    try {
      await invoke("stop_interpretation");
      isRunning = false;
      status = "";
    } catch (e) {
      status = `${e}`;
    }
  }

  function toggleRunning() {
    if (isRunning) stop();
    else start();
  }

  // Track scroll position for sticky auto-scroll
  function onSubtitleScroll() {
    if (!subtitleEl) return;
    const threshold = 50;
    wasAtBottom = subtitleEl.scrollHeight - subtitleEl.scrollTop - subtitleEl.clientHeight < threshold;
  }

  // Sticky auto-scroll: only scroll to bottom if user was already at the bottom.
  // This lets users scroll up to read history without being yanked back down.
  $effect(() => {
    if (subtitles.length || currentSource || currentTranslation) {
      if (wasAtBottom) {
        requestAnimationFrame(() => {
          if (subtitleEl) subtitleEl.scrollTop = subtitleEl.scrollHeight;
        });
      }
    }
  });

  $effect(() => {
    loadSettings();

    // Reload settings when settings window saves and closes
    const unlistenPromise = listen("settings-changed", () => { loadSettings(); });

    // Ensure audio capture is stopped when window closes
    const cleanup = () => {
      if (isRunning) {
        invoke("stop_interpretation").catch(() => {});
      }
    };
    window.addEventListener("beforeunload", cleanup);
    return () => {
      window.removeEventListener("beforeunload", cleanup);
      unlistenPromise.then(fn => fn());
    };
  });

  const languages = [
    { code: "zh", name: "中文" },
    { code: "en", name: "EN" },
    { code: "ja", name: "日本語" },
    { code: "de", name: "DE" },
    { code: "fr", name: "FR" },
    { code: "es", name: "ES" },
    { code: "pt", name: "PT" },
    { code: "id", name: "ID" },
  ];


</script>

<main>
  <!-- Top bar -->
  <header>
    <div class="lang-pair" class:disabled={isRunning}>
      <select bind:value={sourceLang} disabled={isRunning}>
        {#each languages as lang}
          <option value={lang.code}>{lang.name}</option>
        {/each}
      </select>
      <svg class="arrow-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M5 12h14M12 5l7 7-7 7" />
      </svg>
      <select bind:value={targetLang} disabled={isRunning}>
        {#each languages as lang}
          <option value={lang.code}>{lang.name}</option>
        {/each}
      </select>
    </div>
    <button class="icon-btn" onclick={openSettings} title={t(uiLang, "settings")}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M12 15a3 3 0 100-6 3 3 0 000 6z" />
        <path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 01-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 008.7 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06a1.65 1.65 0 00.33-1.82 1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 8.7a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06a1.65 1.65 0 001.82.33H9a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06a1.65 1.65 0 00-.33 1.82V9c.26.604.852.997 1.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z" />
      </svg>
    </button>
  </header>

  <!-- Subtitle area -->
  <section class="subtitles" bind:this={subtitleEl} onscroll={onSubtitleScroll}>
    {#if subtitles.length === 0 && !currentSource && !currentTranslation && !isRunning}
      <div class="empty-state">
        <div class="empty-icon">
          <svg viewBox="0 0 48 48" fill="none" stroke="currentColor" stroke-width="1.5">
            <circle cx="24" cy="24" r="20" />
            <path d="M16 20c0-4.4 3.6-8 8-8s8 3.6 8 8v4c0 4.4-3.6 8-8 8s-8-3.6-8-8v-4z" />
            <path d="M24 32v4M20 36h8" />
          </svg>
        </div>
        <p>{t(uiLang, "emptyHint")}</p>
      </div>
    {/if}
    {#each subtitles as pair}
      <div class="subtitle-pair">
        <p class="source">{pair.source}</p>
        <p class="translation">{pair.translation}</p>
      </div>
    {/each}
    {#if currentSource || currentTranslation}
      <div class="subtitle-pair current">
        <p class="source">{currentSource}</p>
        <p class="translation">{currentTranslation}</p>
      </div>
    {/if}
  </section>

  <!-- Bottom controls -->
  <footer>
    {#if status}
      <span class="status" class:error={status.length > 0 && !isRunning}>{status}</span>
    {/if}
    <button
      class="main-btn"
      class:running={isRunning}
      onclick={toggleRunning}
    >
      {#if isRunning}
        <div class="btn-inner running-inner">
          <svg viewBox="0 0 24 24" fill="currentColor">
            <rect x="7" y="7" width="10" height="10" rx="2" />
          </svg>
          <span>{t(uiLang, "stop")}</span>
        </div>
      {:else}
        <div class="btn-inner">
          <div class="mic-dot"></div>
          <span>{t(uiLang, "start")}</span>
        </div>
      {/if}
    </button>
  </footer>

</main>

<style>
  :global(html), :global(body) {
    overflow: hidden;
    margin: 0;
    padding: 0;
  }

  :global(::-webkit-scrollbar) {
    display: none;
  }

  :root {
    --bg: #0a0a0f;
    --surface: #141419;
    --border: #1e1e26;
    --text: #e8e8ed;
    --text-muted: #6b6b7b;
    --accent: #e0e0e0;
    --accent-hover: #ffffff;
    --danger: #ff5050;
    --radius: 12px;
    font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif;
    font-size: 14px;
    color: var(--text);
    background: var(--bg);
  }

  main {
    height: 100vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* Header */
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }

  .lang-pair {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .lang-pair.disabled {
    opacity: 0.5;
    pointer-events: none;
  }

  .lang-pair select {
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text);
    padding: 6px 10px;
    border-radius: 8px;
    font-size: 13px;
    cursor: pointer;
    outline: none;
  }

  .lang-pair select:focus {
    border-color: var(--accent);
  }

  .arrow-icon {
    width: 16px;
    height: 16px;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .icon-btn {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    border-radius: 8px;
    padding: 4px;
    transition: color 0.2s, background 0.2s;
  }

  .icon-btn:hover {
    color: var(--text);
    background: var(--surface);
  }

  .icon-btn svg {
    width: 20px;
    height: 20px;
  }

  /* Subtitles */
  .subtitles {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
    scroll-behavior: smooth;
  }

  /* scrollbar hidden globally, subtitles still scrollable */

  .empty-state {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    gap: 16px;
  }

  .empty-icon svg {
    width: 48px;
    height: 48px;
    opacity: 0.3;
  }

  .empty-state p {
    font-size: 13px;
    margin: 0;
  }

  .subtitle-pair {
    padding: 10px 0;
  }

  .subtitle-pair + .subtitle-pair {
    border-top: 1px solid var(--border);
  }

  .subtitle-pair.current {
    opacity: 0.5;
  }

  .subtitle-pair p {
    margin: 0;
    line-height: 1.6;
  }

  .source {
    color: var(--text-muted);
    font-size: 13px;
  }

  .translation {
    color: var(--text);
    font-size: 15px;
    margin-top: 2px;
  }

  /* Footer */
  footer {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 16px;
    gap: 8px;
    border-top: 1px solid var(--border);
  }

  .status {
    font-size: 12px;
    color: var(--text-muted);
  }

  .status.error {
    color: var(--danger);
  }

  .main-btn {
    width: 100%;
    max-width: 200px;
    padding: 14px 24px;
    border-radius: 28px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    background: rgba(255, 255, 255, 0.08);
    color: var(--text);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
    font-size: 15px;
    font-weight: 500;
  }

  .main-btn:hover {
    background: rgba(255, 255, 255, 0.14);
    border-color: rgba(255, 255, 255, 0.25);
  }

  .main-btn:active {
    transform: scale(0.98);
  }

  .main-btn.running {
    background: rgba(255, 80, 80, 0.1);
    border-color: rgba(255, 80, 80, 0.25);
    color: var(--danger);
  }

  .main-btn.running:hover {
    background: rgba(255, 80, 80, 0.2);
  }

  .btn-inner {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .btn-inner svg {
    width: 18px;
    height: 18px;
  }

  .mic-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #4ade80;
    flex-shrink: 0;
  }

  .running-inner svg {
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }

</style>
