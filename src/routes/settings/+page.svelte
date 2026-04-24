<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { emit } from "@tauri-apps/api/event";
  import { detectLocale, t, type Locale } from "$lib/i18n";

  let appVersion = $state("");
  const uiLang: Locale = detectLocale();
  let tab = $state<"general" | "corpus">("general");

  let apiKey = $state("");
  let ttsMode = $state<"off" | "preset" | "clone">("preset");
  let speakerId = $state("zh_female_xiaoai_uranus_bigtts");
  let ttsPlaybackSpeed = $state("1.1");
  let hotWords = $state("");
  let glossary = $state("");
  let correctWords = $state("");

  const voices = [
    { id: "zh_female_vv_uranus_bigtts", key: "voiceFemaleA" },
    { id: "zh_female_xiaoai_uranus_bigtts", key: "voiceFemaleB" },
    { id: "zh_male_jingqiangkanye_emo_mars_bigtts", key: "voiceMale" },
  ];

  async function loadSettings() {
    try { appVersion = await getVersion(); } catch (_) {}
    try {
      const { load } = await import("@tauri-apps/plugin-store");
      const store = await load("settings.json");
      apiKey = (await store.get<string>("api_key")) || "";
      ttsMode = (await store.get<string>("tts_mode") as "off" | "preset" | "clone") || "preset";
      speakerId = (await store.get<string>("speaker_id")) || "zh_female_xiaoai_uranus_bigtts";
      ttsPlaybackSpeed = (await store.get<string>("tts_playback_speed")) || "1.1";
      hotWords = (await store.get<string>("hot_words")) || "";
      glossary = (await store.get<string>("glossary")) || "";
      correctWords = (await store.get<string>("correct_words")) || "";
    } catch (_) {}
  }

  async function saveAndClose() {
    try {
      const { load } = await import("@tauri-apps/plugin-store");
      const store = await load("settings.json");
      await store.set("api_key", apiKey);
      await store.set("tts_mode", ttsMode);
      await store.set("speaker_id", speakerId);
      await store.set("tts_playback_speed", ttsPlaybackSpeed);
      await store.set("hot_words", hotWords);
      await store.set("glossary", glossary);
      await store.set("correct_words", correctWords);
      await store.save();
    } catch (e) {
      console.error("Failed to save:", e);
    }
    await emit("settings-changed");
    getCurrentWindow().close();
  }

  $effect(() => { loadSettings(); });
</script>

<div class="settings-page">
  <div class="tabs">
    <button class="tab" class:active={tab === "general"} onclick={() => tab = "general"}>{t(uiLang, "tabGeneral")}</button>
    <button class="tab" class:active={tab === "corpus"} onclick={() => tab = "corpus"}>{t(uiLang, "tabCorpus")}</button>
  </div>

  <div class="body">
    {#if tab === "general"}
      <div class="api-info">
        <p class="api-desc">{t(uiLang, "apiDescBefore")}<a href="https://console.volcengine.com/speech/new/experience/translate" onclick={(e) => { e.preventDefault(); openUrl("https://console.volcengine.com/speech/new/experience/translate"); }}>{t(uiLang, "apiDescLink")}</a>{t(uiLang, "apiDescAfter")}</p>
        <p class="api-note">{t(uiLang, "apiNote")}</p>
      </div>

      <div class="field">
        <label for="api-key">API Key</label>
        <input id="api-key" type="password" bind:value={apiKey} placeholder={t(uiLang, "apiKeyPlaceholder")} />
      </div>

      <div class="divider"></div>

      <div class="field">
        <label for="tts-mode">{t(uiLang, "voiceTts")}</label>
        <select id="tts-mode" bind:value={ttsMode}>
          <option value="off">{t(uiLang, "ttsOff")}</option>
          <option value="preset">{t(uiLang, "ttsPreset")}</option>
          <option value="clone">{t(uiLang, "ttsClone")}</option>
        </select>
      </div>

      {#if ttsMode === "preset"}
        <div class="field">
          <label>{t(uiLang, "voiceSelect")}</label>
          <div class="voice-group">
            {#each voices as v}
              <button class="voice-chip" class:active={speakerId === v.id} onclick={() => (speakerId = v.id)}>{t(uiLang, v.key)}</button>
            {/each}
          </div>
        </div>
      {/if}

      {#if ttsMode !== "off"}
        <div class="field">
          <label for="tts-playback-speed">播放语速</label>
          <select id="tts-playback-speed" bind:value={ttsPlaybackSpeed}>
            <option value="1.0">正常 (1.0x)</option>
            <option value="1.1">稍快 (1.1x)</option>
            <option value="1.2">快速 (1.2x)</option>
          </select>
        </div>
      {/if}
    {:else}
      <div class="corpus-desc">{t(uiLang, "corpusDesc")}</div>

      <div class="field">
        <label>{t(uiLang, "hotWordsLabel")}</label>
        <p class="field-help">{t(uiLang, "hotWordsHelp")}</p>
        <textarea class="corpus-input" bind:value={hotWords} placeholder={t(uiLang, "hotWordsPlaceholder")} rows="4"></textarea>
      </div>

      <div class="field">
        <label>{t(uiLang, "glossaryLabel")}</label>
        <p class="field-help">{t(uiLang, "glossaryHelp")}</p>
        <textarea class="corpus-input" bind:value={glossary} placeholder={t(uiLang, "glossaryPlaceholder")} rows="4"></textarea>
      </div>

      <div class="field">
        <label>{t(uiLang, "correctWordsLabel")}</label>
        <p class="field-help">{t(uiLang, "correctWordsHelp")}</p>
        <textarea class="corpus-input" bind:value={correctWords} placeholder={t(uiLang, "correctWordsPlaceholder")} rows="4"></textarea>
      </div>
    {/if}
  </div>

  <div class="footer">
    <button class="save-btn" onclick={saveAndClose} disabled={!apiKey}>{t(uiLang, "done")}</button>
    <p class="copyright">{#if appVersion}v{appVersion} · {/if}by wangxin · <a href="https://github.com/wxkingstar/TransEcho" onclick={(e) => { e.preventDefault(); openUrl("https://github.com/wxkingstar/TransEcho"); }}>GitHub</a></p>
  </div>
</div>

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
    --radius: 12px;
    font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif;
    font-size: 14px;
    color: var(--text);
    background: var(--surface);
  }

  .settings-page {
    height: 100vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .tabs {
    display: flex;
    border-bottom: 1px solid var(--border);
    padding: 0 24px;
  }

  .tab {
    flex: 1;
    padding: 14px 0;
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--text-muted);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: color 0.2s, border-color 0.2s;
    text-align: center;
  }

  .tab:hover { color: var(--text); }

  .tab.active {
    color: var(--text);
    border-bottom-color: var(--accent);
  }

  .body {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .footer {
    padding: 16px 24px 20px;
    border-top: 1px solid var(--border);
  }

  .api-info {
    padding: 12px 14px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: 10px;
    border: 1px solid var(--border);
  }

  .api-desc {
    margin: 0;
    font-size: 13px;
    color: var(--text-muted);
    line-height: 1.5;
  }

  .api-desc a {
    color: var(--text);
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .api-desc a:hover { color: var(--accent-hover); }

  .api-note {
    margin: 6px 0 0;
    font-size: 12px;
    color: #4ade80;
    opacity: 0.85;
  }

  .field label {
    display: block;
    font-size: 12px;
    color: var(--text-muted);
    margin-bottom: 8px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .field input[type="password"] {
    width: 100%;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    font-size: 15px;
    box-sizing: border-box;
    outline: none;
    transition: border-color 0.2s;
  }

  .field input:focus { border-color: var(--accent); }

  .divider {
    height: 1px;
    background: var(--border);
  }

  .field select {
    width: 100%;
    padding: 12px 16px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    font-size: 14px;
    box-sizing: border-box;
    outline: none;
    cursor: pointer;
    transition: border-color 0.2s;
    -webkit-appearance: none;
    appearance: none;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%236b6b7b' stroke-width='2'%3E%3Cpath d='M6 9l6 6 6-6'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 14px center;
  }

  .field select:focus {
    border-color: var(--accent);
  }

  .voice-group {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }

  .voice-chip {
    flex: 1;
    padding: 10px 0;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text-muted);
    font-size: 13px;
    cursor: pointer;
    transition: all 0.2s;
    text-align: center;
  }

  .voice-chip:hover {
    border-color: var(--text-muted);
    color: var(--text);
  }

  .voice-chip.active {
    border-color: rgba(255, 255, 255, 0.3);
    background: rgba(255, 255, 255, 0.08);
    color: var(--text);
  }

  .corpus-desc {
    font-size: 13px;
    color: var(--text-muted);
    line-height: 1.6;
    padding: 12px 14px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: 10px;
    border: 1px solid var(--border);
  }

  .field-help {
    margin: 0 0 6px;
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.5;
    opacity: 0.8;
  }

  .corpus-input {
    width: 100%;
    padding: 10px 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    font-size: 13px;
    font-family: inherit;
    resize: vertical;
    min-height: 40px;
    box-sizing: border-box;
    outline: none;
    transition: border-color 0.2s;
  }

  .corpus-input:focus { border-color: var(--accent); }

  .corpus-input::placeholder {
    color: var(--text-muted);
    opacity: 0.5;
  }

  .save-btn {
    width: 100%;
    padding: 14px;
    border: none;
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.1);
    color: var(--text);
    font-size: 15px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.2s;
  }

  .save-btn:hover { background: rgba(255, 255, 255, 0.16); }
  .save-btn:disabled { opacity: 0.3; pointer-events: none; }

  .copyright {
    margin: 12px 0 0;
    text-align: center;
    font-size: 11px;
    color: var(--text-muted);
    opacity: 0.5;
  }

  .copyright a {
    color: var(--text-muted);
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .copyright a:hover { color: var(--text); }
</style>
