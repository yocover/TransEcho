<script lang="ts">
  import { invoke, Channel, isTauri } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { listen } from "@tauri-apps/api/event";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { detectLocale, t, translateStatus, type Locale } from "$lib/i18n";

  type AudioDevice = {
    id: string;
    name: string;
    is_default: boolean;
    is_input: boolean;
    is_output: boolean;
    channels?: number | null;
    sample_rate?: number | null;
  };

  type AudioDeviceList = {
    inputs: AudioDevice[];
    outputs: AudioDevice[];
  };

  let appVersion = $state("");
  const uiLang: Locale = detectLocale();
  const inTauri = isTauri();

  let apiKey = $state("");
  let sourceLang = $state("zh");
  let targetLang = $state("en");
  let speakerId = $state("zh_female_xiaoai_uranus_bigtts");
  let hotWords = $state("");
  let glossary = $state("");
  let correctWords = $state("");

  let inputDevices: AudioDevice[] = $state([]);
  let outputDevices: AudioDevice[] = $state([]);
  let inputDeviceName = $state("");
  let outputDeviceName = $state("");
  let monitorEnabled = $state(false);
  let monitorDeviceName = $state("");

  let isRunning = $state(false);
  let isLoadingDevices = $state(false);
  let status = $state("");
  let deviceStatus = $state("");
  let needsMicPermissionGuide = $state(false);
  let needsBlackHoleGuide = $state(false);
  let subtitles: Array<{ source: string; translation: string }> = $state([]);
  let currentSource = $state("");
  let currentTranslation = $state("");
  let subtitleEl: HTMLElement;
  let wasAtBottom = $state(true);

  const voices = [
    { id: "zh_female_vv_uranus_bigtts", key: "voiceFemaleA" },
    { id: "zh_female_xiaoai_uranus_bigtts", key: "voiceFemaleB" },
    { id: "zh_male_jingqiangkanye_emo_mars_bigtts", key: "voiceMale" },
  ];

  function parseCorpus() {
    const hotWordsList = hotWords.split("\n").map((s) => s.trim()).filter(Boolean);
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
    return {
      hotWordsList,
      glossaryMap,
      correctWordsJson: Object.keys(correctWordsMap).length > 0 ? JSON.stringify(correctWordsMap) : "",
    };
  }

  function pickPreferredInput(devices: AudioDevice[], saved: string) {
    return (
      devices.find((device) => device.name === saved)?.name ||
      devices.find((device) => device.is_default)?.name ||
      devices[0]?.name ||
      ""
    );
  }

  function pickPreferredOutput(devices: AudioDevice[], saved: string) {
    return (
      devices.find((device) => device.name === saved)?.name ||
      devices.find((device) => device.name.toLowerCase().includes("blackhole"))?.name ||
      devices.find((device) => device.is_default)?.name ||
      devices[0]?.name ||
      ""
    );
  }

  function pickPreferredMonitor(devices: AudioDevice[], saved: string, primary: string) {
    return (
      devices.find((device) => device.name === saved && device.name !== primary)?.name ||
      devices.find((device) => device.is_default && device.name !== primary)?.name ||
      devices.find(
        (device) =>
          !device.name.toLowerCase().includes("blackhole") && device.name !== primary
      )?.name ||
      devices.find((device) => device.name !== primary)?.name ||
      ""
    );
  }

  function updateDeviceHealth(inputs: AudioDevice[], outputs: AudioDevice[]) {
    const hasInput = inputs.length > 0;
    const hasBlackHole = outputs.some((device) =>
      device.name.toLowerCase().includes("blackhole")
    );

    needsMicPermissionGuide = !hasInput;
    needsBlackHoleGuide = !hasBlackHole;

    if (!hasInput) {
      deviceStatus = t(uiLang, "inputDeviceMissing");
      return;
    }

    if (!hasBlackHole) {
      deviceStatus = t(uiLang, "blackHoleMissing");
      return;
    }

    deviceStatus = t(uiLang, "deviceCheckOk");
  }

  async function loadSettings() {
    if (!inTauri) return;
    try {
      appVersion = await getVersion();
    } catch (_) {}

    try {
      const { load } = await import("@tauri-apps/plugin-store");
      const store = await load("settings.json");
      apiKey = (await store.get<string>("api_key")) || "";
      sourceLang = (await store.get<string>("bridge_source_lang")) || "zh";
      targetLang = (await store.get<string>("bridge_target_lang")) || "en";
      speakerId = (await store.get<string>("speaker_id")) || "zh_female_xiaoai_uranus_bigtts";
      hotWords = (await store.get<string>("hot_words")) || "";
      glossary = (await store.get<string>("glossary")) || "";
      correctWords = (await store.get<string>("correct_words")) || "";
      inputDeviceName = (await store.get<string>("input_device_name")) || "";
      outputDeviceName = (await store.get<string>("output_device_name")) || "";
      monitorEnabled = (await store.get<boolean>("monitor_enabled")) || false;
      monitorDeviceName = (await store.get<string>("monitor_device_name")) || "";
      if (!apiKey) openSettings();
    } catch (_) {
      openSettings();
    }
  }

  async function saveBridgeSettings() {
    if (!inTauri) return;
    try {
      const { load } = await import("@tauri-apps/plugin-store");
      const store = await load("settings.json");
      await store.set("bridge_source_lang", sourceLang);
      await store.set("bridge_target_lang", targetLang);
      await store.set("speaker_id", speakerId);
      await store.set("input_device_name", inputDeviceName);
      await store.set("output_device_name", outputDeviceName);
      await store.set("monitor_enabled", monitorEnabled);
      await store.set("monitor_device_name", monitorDeviceName);
      await store.save();
    } catch (_) {}
  }

  async function loadAudioDevices() {
    if (!inTauri) return;
    isLoadingDevices = true;
    try {
      const devices = await invoke<AudioDeviceList>("list_audio_devices");
      inputDevices = devices.inputs || [];
      outputDevices = devices.outputs || [];
      inputDeviceName = pickPreferredInput(inputDevices, inputDeviceName);
      outputDeviceName = pickPreferredOutput(outputDevices, outputDeviceName);
      monitorDeviceName = pickPreferredMonitor(outputDevices, monitorDeviceName, outputDeviceName);
      updateDeviceHealth(inputDevices, outputDevices);
    } catch (e) {
      deviceStatus = `${t(uiLang, "deviceCheckFailed")}: ${e}`;
      inputDevices = [];
      outputDevices = [];
      needsMicPermissionGuide = true;
    } finally {
      isLoadingDevices = false;
    }
  }

  async function openSettings() {
    if (!inTauri) {
      window.location.href = "/settings";
      return;
    }
    const existing = await WebviewWindow.getByLabel("settings");
    if (existing) {
      await existing.setFocus();
      return;
    }
    new WebviewWindow("settings", {
      url: "/settings",
      title: t(uiLang, "settings"),
      width: 560,
      height: 780,
      center: true,
      resizable: true,
      minimizable: false,
      maximizable: false,
    });
  }

  async function openMicrophoneSettings() {
    try {
      await openUrl("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone");
    } catch (_) {
      await openUrl("x-apple.systempreferences:");
    }
  }

  async function openBlackHoleGuide() {
    await openUrl("https://github.com/ExistentialAudio/BlackHole");
  }

  function buildSubtitleChannel() {
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
              subtitles = [
                ...subtitles.slice(-19),
                { source: currentSource, translation: currentTranslation },
              ];
              currentSource = "";
              currentTranslation = "";
            }
          } else if (event.text) {
            currentTranslation = event.text;
          }
          break;
        case "status":
          status = event.message ? translateStatus(uiLang, event.message) : "";
          if (event.message === "stopped" || event.message === "session_ended") {
            isRunning = false;
          }
          break;
        case "error":
          status = event.message ? translateStatus(uiLang, event.message) : t(uiLang, "unknown_error");
          isRunning = false;
          needsMicPermissionGuide = true;
          break;
      }
    };

    return onSubtitle;
  }

  async function start() {
    if (!inTauri) {
      status = t(uiLang, "desktopOnly");
      return;
    }

    if (!apiKey) {
      openSettings();
      return;
    }

    if (!inputDeviceName) {
      needsMicPermissionGuide = true;
      status = t(uiLang, "inputDeviceMissing");
      return;
    }

    if (!outputDeviceName) {
      needsBlackHoleGuide = true;
      status = t(uiLang, "outputDeviceMissing");
      return;
    }

    if (monitorEnabled && !monitorDeviceName) {
      status = t(uiLang, "monitorDeviceMissing");
      return;
    }

    if (monitorEnabled && monitorDeviceName === outputDeviceName) {
      status = t(uiLang, "monitorDeviceConflict");
      return;
    }

    const { hotWordsList, glossaryMap, correctWordsJson } = parseCorpus();
    const onSubtitle = buildSubtitleChannel();

    try {
      await saveBridgeSettings();
      isRunning = true;
      status = t(uiLang, "bridgeConnecting");

      await invoke("start_mic_bridge", {
        onSubtitle,
        apiKey,
        inputDeviceName,
        outputDeviceName,
        enableMonitor: monitorEnabled,
        monitorDeviceName,
        sourceLanguage: sourceLang,
        targetLanguage: targetLang,
        speakerId,
        hotWords: hotWordsList,
        glossary: glossaryMap,
        correctWords: correctWordsJson,
      });
    } catch (e) {
      const message = `${e}`;
      status = translateStatus(uiLang, message);
      isRunning = false;
      if (
        message.toLowerCase().includes("input") ||
        message.toLowerCase().includes("microphone") ||
        message.toLowerCase().includes("device")
      ) {
        needsMicPermissionGuide = true;
      }
    }
  }

  async function stop() {
    if (!inTauri) return;
    try {
      await invoke("stop_mic_bridge");
      isRunning = false;
      status = t(uiLang, "bridgeStopped");
    } catch (e) {
      status = `${e}`;
    }
  }

  function toggleRunning() {
    if (isRunning) stop();
    else start();
  }

  function onSubtitleScroll() {
    if (!subtitleEl) return;
    const threshold = 50;
    wasAtBottom = subtitleEl.scrollHeight - subtitleEl.scrollTop - subtitleEl.clientHeight < threshold;
  }

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
    if (monitorDeviceName === outputDeviceName) {
      monitorDeviceName = pickPreferredMonitor(outputDevices, "", outputDeviceName);
    }
  });

  $effect(() => {
    loadSettings();
    loadAudioDevices();

    if (!inTauri) return;

    const unlistenPromise = listen("settings-changed", async () => {
      await loadSettings();
      await loadAudioDevices();
    });

    const cleanup = () => {
      if (isRunning) {
        invoke("stop_mic_bridge").catch(() => {});
      }
    };
    window.addEventListener("beforeunload", cleanup);
    return () => {
      window.removeEventListener("beforeunload", cleanup);
      unlistenPromise.then((fn) => fn());
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
  <header>
    <div>
      <h1>{t(uiLang, "bridgeTitle")}</h1>
      <p>{#if appVersion}v{appVersion} · {/if}{t(uiLang, "bridgeSubtitle")}</p>
    </div>
    <button class="icon-btn" onclick={openSettings} title={t(uiLang, "settings")}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M12 15a3 3 0 100-6 3 3 0 000 6z" />
        <path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 01-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 008.7 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06a1.65 1.65 0 00.33-1.82 1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 8.7a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06a1.65 1.65 0 001.82.33H9a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06a1.65 1.65 0 00-.33 1.82V9c.26.604.852.997 1.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z" />
      </svg>
    </button>
  </header>

  <section class="panel">
    <div class="panel-header">
      <h2>{t(uiLang, "deviceSectionTitle")}</h2>
      <button class="secondary-btn" onclick={loadAudioDevices} disabled={isLoadingDevices || isRunning}>
        {isLoadingDevices ? t(uiLang, "refreshingDevices") : t(uiLang, "refreshDevices")}
      </button>
    </div>

    <div class="device-health">
      <span class:ok={!needsMicPermissionGuide && !needsBlackHoleGuide}>{deviceStatus}</span>
    </div>

    <div class="field-grid">
      <div class="field">
        <label for="input-device">{t(uiLang, "inputDeviceLabel")}</label>
        <select id="input-device" bind:value={inputDeviceName} disabled={isRunning}>
          {#if inputDevices.length === 0}
            <option value="">{t(uiLang, "inputDevicePlaceholder")}</option>
          {/if}
          {#each inputDevices as device}
            <option value={device.name}>
              {device.name}{device.is_default ? ` · ${t(uiLang, "defaultDevice")}` : ""}
            </option>
          {/each}
        </select>
      </div>

      <div class="field">
        <label for="output-device">{t(uiLang, "outputDeviceLabel")}</label>
        <select id="output-device" bind:value={outputDeviceName} disabled={isRunning}>
          {#if outputDevices.length === 0}
            <option value="">{t(uiLang, "outputDevicePlaceholder")}</option>
          {/if}
          {#each outputDevices as device}
            <option value={device.name}>
              {device.name}{device.is_default ? ` · ${t(uiLang, "defaultDevice")}` : ""}
            </option>
          {/each}
        </select>
      </div>
    </div>

    <div class="field-grid">
      <div class="field">
        <label for="source-lang">{t(uiLang, "sourceLanguageLabel")}</label>
        <select id="source-lang" bind:value={sourceLang} disabled={isRunning}>
          {#each languages as lang}
            <option value={lang.code}>{lang.name}</option>
          {/each}
        </select>
      </div>

      <div class="field">
        <label for="target-lang">{t(uiLang, "targetLanguageLabel")}</label>
        <select id="target-lang" bind:value={targetLang} disabled={isRunning}>
          {#each languages as lang}
            <option value={lang.code}>{lang.name}</option>
          {/each}
        </select>
      </div>
    </div>

    <div class="field-grid monitor-grid">
      <div class="field">
        <label for="monitor-enabled">{t(uiLang, "localMonitorLabel")}</label>
        <label class="switch">
          <input id="monitor-enabled" type="checkbox" bind:checked={monitorEnabled} disabled={isRunning} />
          <span>{monitorEnabled ? t(uiLang, "localMonitorOn") : t(uiLang, "localMonitorOff")}</span>
        </label>
      </div>

      <div class="field">
        <label for="monitor-device">{t(uiLang, "monitorDeviceLabel")}</label>
        <select id="monitor-device" bind:value={monitorDeviceName} disabled={isRunning || !monitorEnabled}>
          {#if outputDevices.length === 0}
            <option value="">{t(uiLang, "monitorDevicePlaceholder")}</option>
          {/if}
          {#each outputDevices.filter((device) => device.name !== outputDeviceName) as device}
            <option value={device.name}>
              {device.name}{device.is_default ? ` · ${t(uiLang, "defaultDevice")}` : ""}
            </option>
          {/each}
        </select>
      </div>
    </div>

    <div class="field">
      <div class="field-title">{t(uiLang, "voiceSelect")}</div>
      <div class="voice-group">
        {#each voices as v}
          <button class="voice-chip" class:active={speakerId === v.id} onclick={() => (speakerId = v.id)} disabled={isRunning}>
            {t(uiLang, v.key)}
          </button>
        {/each}
      </div>
    </div>
  </section>

  {#if needsMicPermissionGuide}
    <section class="notice warning">
      <div>
        <strong>{t(uiLang, "micPermissionTitle")}</strong>
        <p>{t(uiLang, "micPermissionHint")}</p>
      </div>
      <button class="secondary-btn" onclick={openMicrophoneSettings}>{t(uiLang, "openMicSettings")}</button>
    </section>
  {/if}

  {#if needsBlackHoleGuide}
    <section class="notice">
      <div>
        <strong>{t(uiLang, "blackHoleTitle")}</strong>
        <p>{t(uiLang, "blackHoleHint")}</p>
      </div>
      <button class="secondary-btn" onclick={openBlackHoleGuide}>{t(uiLang, "viewBlackHoleGuide")}</button>
    </section>
  {/if}

  <section class="subtitles" bind:this={subtitleEl} onscroll={onSubtitleScroll}>
    <div class="subtitle-head">
      <h2>{t(uiLang, "debugSubtitles")}</h2>
      <span class="status-text">{status}</span>
    </div>
    {#if subtitles.length === 0 && !currentSource && !currentTranslation}
      <div class="empty-state">
        <p>{t(uiLang, "bridgeEmptyHint")}</p>
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

  <footer>
    <button class="main-btn" class:running={isRunning} onclick={toggleRunning}>
      {#if isRunning}
        <div class="btn-inner running-inner">
          <svg viewBox="0 0 24 24" fill="currentColor">
            <rect x="7" y="7" width="10" height="10" rx="2" />
          </svg>
          <span>{t(uiLang, "stopBridge")}</span>
        </div>
      {:else}
        <div class="btn-inner">
          <div class="mic-dot"></div>
          <span>{t(uiLang, "startBridge")}</span>
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
    --warning: #f59e0b;
    --ok: #4ade80;
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
    gap: 12px;
    padding: 12px;
    box-sizing: border-box;
    overflow: hidden;
  }

  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  h1, h2 {
    margin: 0;
  }

  header h1 {
    font-size: 20px;
  }

  header p {
    margin: 4px 0 0;
    color: var(--text-muted);
    font-size: 12px;
  }

  .panel, .notice, .subtitles {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .panel {
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .panel-header, .subtitle-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .device-health {
    font-size: 12px;
    color: var(--warning);
  }

  .device-health .ok {
    color: var(--ok);
  }

  .field-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .field label {
    display: block;
    font-size: 12px;
    color: var(--text-muted);
    margin-bottom: 6px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .field-title {
    display: block;
    font-size: 12px;
    color: var(--text-muted);
    margin-bottom: 6px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .field select {
    width: 100%;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    font-size: 14px;
    box-sizing: border-box;
    outline: none;
    cursor: pointer;
  }

  .field select:focus {
    border-color: var(--accent);
  }

  .voice-group {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .voice-chip, .secondary-btn, .icon-btn {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    border-radius: 10px;
    cursor: pointer;
    transition: background 0.2s, border-color 0.2s, color 0.2s;
  }

  .switch {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 0 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    cursor: pointer;
    user-select: none;
  }

  .switch input {
    width: 16px;
    height: 16px;
    accent-color: var(--ok);
  }

  .voice-chip {
    padding: 8px 12px;
  }

  .voice-chip.active {
    border-color: var(--accent);
    background: rgba(255,255,255,0.08);
  }

  .secondary-btn {
    padding: 8px 12px;
    font-size: 12px;
  }

  .secondary-btn:hover,
  .voice-chip:hover,
  .icon-btn:hover {
    border-color: var(--accent);
  }

  .notice {
    padding: 12px 14px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .notice.warning {
    border-color: rgba(245, 158, 11, 0.35);
  }

  .notice strong {
    display: block;
    margin-bottom: 4px;
  }

  .notice p {
    margin: 0;
    color: var(--text-muted);
    font-size: 12px;
    line-height: 1.5;
  }

  .subtitles {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 14px;
  }

  .status-text {
    font-size: 12px;
    color: var(--text-muted);
  }

  .empty-state {
    color: var(--text-muted);
    font-size: 13px;
    padding: 20px 0;
  }

  .subtitle-pair {
    padding: 10px 0;
  }

  .subtitle-pair + .subtitle-pair {
    border-top: 1px solid var(--border);
  }

  .subtitle-pair.current {
    opacity: 0.6;
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
  }

  footer {
    display: flex;
    justify-content: center;
  }

  .main-btn {
    width: 100%;
    padding: 14px 24px;
    border-radius: 28px;
    border: 1px solid rgba(255,255,255,0.15);
    background: linear-gradient(180deg, #20202a 0%, #17171f 100%);
    color: var(--text);
    cursor: pointer;
  }

  .main-btn.running {
    border-color: rgba(255,80,80,0.35);
  }

  .btn-inner {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    font-size: 15px;
    font-weight: 600;
  }

  .running-inner svg {
    width: 16px;
    height: 16px;
  }

  .mic-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--ok);
    box-shadow: 0 0 0 6px rgba(74, 222, 128, 0.12);
  }

  .icon-btn {
    width: 36px;
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }

  .icon-btn svg {
    width: 20px;
    height: 20px;
  }
</style>
