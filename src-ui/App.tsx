import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";

type Theme = "dark" | "light";
type DeckId = "a" | "b";
type Track = { id: number; path: string; title: string; artist: string | null; durationSeconds: number | null; codec: string | null; missing: boolean };
type ScanResult = { rootPath: string; discovered: number; inserted: number; modified: number; restored: number; unchanged: number; missing: number; metadataFailures: number; traversalErrors: string[] };
type DeckSnapshot = { loadedTrackId: number | null; title: string | null; path: string | null; durationSeconds: number | null; positionSeconds: number; sampleRate: number | null; channels: number | null; state: "paused" | "playing" | "ended"; callbacks: number; underflowCallbacks: number; staleBlocks: number; recycleFailures: number; streamErrors: number; workerError: string | null };
type MixerSnapshot = { deckA: DeckSnapshot; deckB: DeckSnapshot; crossfader: number; channelGainA: number; channelGainB: number; masterGain: number; callbacks: number; clippedSamples: number; streamErrors: number; outputDeviceId: string | null; outputDeviceName: string | null; deviceRecoveries: number; deviceMessage: string | null; cueA: boolean; cueB: boolean; cueBlend: number; cueGain: number; cueSupported: boolean; routingMode: string; routingLimitation: string | null };
type OutputDevice = { id: string; name: string; isDefault: boolean; interface: string; channels: number; maxChannels: number; sampleRate: number; stereoMasterSupported: boolean; stereoCueSupported: boolean; routingMode: string; limitation: string | null };

const unloadedDeck: DeckSnapshot = { loadedTrackId: null, title: null, path: null, durationSeconds: null, positionSeconds: 0, sampleRate: null, channels: null, state: "paused", callbacks: 0, underflowCallbacks: 0, staleBlocks: 0, recycleFailures: 0, streamErrors: 0, workerError: null };
const unloadedMixer: MixerSnapshot = { deckA: unloadedDeck, deckB: unloadedDeck, crossfader: 0, channelGainA: 1, channelGainB: 1, masterGain: 1, callbacks: 0, clippedSamples: 0, streamErrors: 0, outputDeviceId: null, outputDeviceName: null, deviceRecoveries: 0, deviceMessage: null, cueA: false, cueB: false, cueBlend: -1, cueGain: 0.5, cueSupported: false, routingMode: "master-only", routingLimitation: "Load a track to detect cue capability." };

const formatDuration = (seconds: number | null) => {
  if (seconds === null || !Number.isFinite(seconds)) return "--:--";
  const whole = Math.max(0, Math.round(seconds));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}`;
};

type DeckPanelProps = { id: DeckId; snapshot: DeckSnapshot; seek: number; busy: boolean; message: string; onSeekChange: (value: number) => void; onCommand: (command: string, args?: Record<string, unknown>, success?: string) => void };

function DeckPanel({ id, snapshot, seek, busy, message, onSeekChange, onCommand }: DeckPanelProps) {
  const label = id.toUpperCase();
  const prefix = `deck_${id}`;
  const commitSeek = () => onCommand(`${prefix}_seek`, { seconds: seek, resume: snapshot.state === "playing" });
  return (
    <article className={`deck-placeholder deck-${id}`}>
      <div className="deck-heading"><span>Deck {label}</span><strong className={`deck-state deck-state-${snapshot.state}`}>{snapshot.state}</strong></div>
      <div className="deck-track"><strong>{snapshot.title ?? "No track loaded"}</strong><p>{formatDuration(snapshot.positionSeconds)} / {formatDuration(snapshot.durationSeconds)}</p></div>
      <input className="seek-control" type="range" min={0} max={Math.max(snapshot.durationSeconds ?? 0, 1)} step={0.1} value={Math.min(seek, Math.max(snapshot.durationSeconds ?? 0, 1))} onChange={(event) => onSeekChange(Number(event.target.value))} onPointerUp={commitSeek} onKeyUp={commitSeek} disabled={snapshot.loadedTrackId === null || busy} aria-label={`Deck ${label} position`} />
      <div className="deck-controls">
        <button type="button" onClick={() => onCommand(`${prefix}_play`)} disabled={snapshot.loadedTrackId === null || busy || snapshot.state === "playing"}>Play</button>
        <button type="button" onClick={() => onCommand(`${prefix}_pause`)} disabled={snapshot.loadedTrackId === null || busy || snapshot.state !== "playing"}>Pause</button>
        <button type="button" onClick={() => onCommand(`${prefix}_stop`, undefined, `Deck ${label} stopped.`)} disabled={snapshot.loadedTrackId === null || busy}>Stop</button>
      </div>
      <p className="deck-message" aria-live="polite">{snapshot.workerError ?? message}</p>
      <small className="deck-health">Underflows {snapshot.underflowCallbacks} · Stream errors {snapshot.streamErrors}</small>
    </article>
  );
}

function App() {
  const [theme, setTheme] = useState<Theme>("dark");
  const [engineStatus, setEngineStatus] = useState("Connecting to Rust engine...");
  const [tracks, setTracks] = useState<Track[]>([]);
  const [libraryStatus, setLibraryStatus] = useState("No music folders scanned yet.");
  const [isScanning, setIsScanning] = useState(false);
  const [mixer, setMixer] = useState<MixerSnapshot>(unloadedMixer);
  const [seekA, setSeekA] = useState(0);
  const [seekB, setSeekB] = useState(0);
  const [messageA, setMessageA] = useState("Choose a ready library track to load deck A.");
  const [messageB, setMessageB] = useState("Choose a ready library track to load deck B.");
  const [busyDeck, setBusyDeck] = useState<DeckId | null>(null);
  const [outputDevices, setOutputDevices] = useState<OutputDevice[]>([]);
  const [deviceStatus, setDeviceStatus] = useState("Discovering audio outputs...");
  const [isSwitchingDevice, setIsSwitchingDevice] = useState(false);

  useEffect(() => { document.documentElement.dataset.theme = theme; }, [theme]);
  useEffect(() => { invoke<string>("engine_status").then(setEngineStatus).catch((error: unknown) => setEngineStatus(`Engine unavailable: ${String(error)}`)); }, []);
  useEffect(() => { refreshLibrary(); }, []);
  useEffect(() => {
    refreshOutputDevices();
    const timer = window.setInterval(() => refreshOutputDevices(true), 3000);
    return () => window.clearInterval(timer);
  }, []);
  useEffect(() => {
    let cancelled = false;
    const refresh = async () => {
      try {
        const snapshot = await invoke<MixerSnapshot>("mixer_snapshot");
        if (!cancelled) { setMixer(snapshot); setSeekA(snapshot.deckA.positionSeconds); setSeekB(snapshot.deckB.positionSeconds); }
      } catch (error: unknown) { if (!cancelled) setMessageA(`Mixer status unavailable: ${String(error)}`); }
    };
    refresh();
    const timer = window.setInterval(refresh, 500);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, []);

  const refreshLibrary = async () => { try { setTracks(await invoke<Track[]>("library_tracks")); } catch (error: unknown) { setLibraryStatus(`Could not load library: ${String(error)}`); } };
  const refreshOutputDevices = async (silent = false) => {
    try {
      const devices = await invoke<OutputDevice[]>("audio_output_devices");
      setOutputDevices(devices);
      if (!silent) setDeviceStatus(devices.length ? `${devices.length} output device${devices.length === 1 ? "" : "s"} available.` : "No audio output devices are available.");
    } catch (error: unknown) { if (!silent) setDeviceStatus(`Could not discover outputs: ${String(error)}`); }
  };
  const applySnapshot = (snapshot: MixerSnapshot) => { setMixer(snapshot); setSeekA(snapshot.deckA.positionSeconds); setSeekB(snapshot.deckB.positionSeconds); };

  const addMusicFolder = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (!selected) return;
    setIsScanning(true); setLibraryStatus(`Scanning ${selected}...`);
    try {
      const result = await invoke<ScanResult>("scan_music_folder", { path: selected });
      const warning = result.traversalErrors.length ? ` ${result.traversalErrors.length} folder entries could not be read; missing-file reconciliation was skipped.` : "";
      setLibraryStatus(`Found ${result.discovered} tracks: ${result.inserted} new, ${result.modified} changed, ${result.restored} restored, ${result.missing} missing, ${result.metadataFailures} metadata failures.${warning}`);
      await refreshLibrary();
    } catch (error: unknown) { setLibraryStatus(`Scan failed: ${String(error)}`); } finally { setIsScanning(false); }
  };

  const runDeckCommand = async (id: DeckId, command: string, args?: Record<string, unknown>, success?: string) => {
    setBusyDeck(id);
    try {
      const snapshot = await invoke<MixerSnapshot>(command, args); applySnapshot(snapshot);
      const deck = id === "a" ? snapshot.deckA : snapshot.deckB;
      const message = success ?? `Deck ${id.toUpperCase()} is ${deck.state}.`;
      id === "a" ? setMessageA(message) : setMessageB(message);
    } catch (error: unknown) { const message = `Deck ${id.toUpperCase()} error: ${String(error)}`; id === "a" ? setMessageA(message) : setMessageB(message); } finally { setBusyDeck(null); }
  };

  const setMixerControl = async (command: string, args: Record<string, unknown>) => {
    try { applySnapshot(await invoke<MixerSnapshot>(command, args)); } catch (error: unknown) { setMessageA(`Mixer error: ${String(error)}`); }
  };

  const selectOutputDevice = async (deviceId: string) => {
    setIsSwitchingDevice(true);
    setDeviceStatus("Switching audio output and restoring deck state...");
    try {
      const snapshot = await invoke<MixerSnapshot>("audio_select_output_device", { deviceId });
      applySnapshot(snapshot);
      setDeviceStatus(snapshot.deviceMessage ?? `Output changed to ${snapshot.outputDeviceName ?? "selected device"}.`);
    } catch (error: unknown) { setDeviceStatus(`Output change failed: ${String(error)}`); } finally { setIsSwitchingDevice(false); }
  };

  return (
    <main className="app-shell">
      <header className="app-header"><div><p className="eyebrow">Offline macOS DJ workstation</p><h1>DJ App</h1></div><button className="theme-toggle" type="button" onClick={() => setTheme(theme === "dark" ? "light" : "dark")} aria-label={`Switch to ${theme === "dark" ? "light" : "dark"} theme`}>{theme === "dark" ? "Light" : "Dark"} theme</button></header>
      <section className="status-card" aria-live="polite">
        <span className="status-dot" aria-hidden="true" />
        <div className="engine-status"><strong>Shared two-deck engine ready</strong><p>{engineStatus}</p></div>
        <div className="output-control">
          <label htmlFor="output-device">Master output</label>
          <select id="output-device" value={mixer.outputDeviceId ?? ""} onChange={(event) => selectOutputDevice(event.target.value)} disabled={isSwitchingDevice || outputDevices.length === 0}>
            {mixer.outputDeviceId === null && <option value="">Select an output</option>}
            {outputDevices.map((device) => <option key={device.id} value={device.id}>{device.name}{device.isDefault ? " (macOS default)" : ""} · {device.maxChannels}ch max · {Math.round(device.sampleRate / 100) / 10} kHz</option>)}
          </select>
          <small>{mixer.deviceMessage ?? deviceStatus}</small>
          <small className="latency-warning">Bluetooth and wireless outputs add latency and are not recommended for beat mixing.</small>
        </div>
      </section>
      <section className="workspace" aria-label="DJ workspace">
        <DeckPanel id="a" snapshot={mixer.deckA} seek={seekA} busy={busyDeck === "a"} message={messageA} onSeekChange={setSeekA} onCommand={(command, args, success) => runDeckCommand("a", command, args, success)} />
        <div className="mixer-placeholder" aria-label="Mixer controls">
          <strong>Mixer</strong>
          <label>Deck A gain<input type="range" min={0} max={1} step={0.01} value={mixer.channelGainA} onChange={(event) => setMixer({ ...mixer, channelGainA: Number(event.target.value) })} onPointerUp={() => setMixerControl("deck_a_set_gain", { gain: mixer.channelGainA })} onKeyUp={() => setMixerControl("deck_a_set_gain", { gain: mixer.channelGainA })} disabled={mixer.deckA.loadedTrackId === null} /></label>
          <label>Master<input type="range" min={0} max={1} step={0.01} value={mixer.masterGain} onChange={(event) => setMixer({ ...mixer, masterGain: Number(event.target.value) })} onPointerUp={() => setMixerControl("mixer_set_master_gain", { gain: mixer.masterGain })} onKeyUp={() => setMixerControl("mixer_set_master_gain", { gain: mixer.masterGain })} disabled={mixer.deckA.loadedTrackId === null && mixer.deckB.loadedTrackId === null} /></label>
          <label>Deck B gain<input type="range" min={0} max={1} step={0.01} value={mixer.channelGainB} onChange={(event) => setMixer({ ...mixer, channelGainB: Number(event.target.value) })} onPointerUp={() => setMixerControl("deck_b_set_gain", { gain: mixer.channelGainB })} onKeyUp={() => setMixerControl("deck_b_set_gain", { gain: mixer.channelGainB })} disabled={mixer.deckB.loadedTrackId === null} /></label>
          <label className="crossfader-label">Crossfader<input type="range" min={-1} max={1} step={0.01} value={mixer.crossfader} onChange={(event) => setMixer({ ...mixer, crossfader: Number(event.target.value) })} onPointerUp={() => setMixerControl("mixer_set_crossfader", { value: mixer.crossfader })} onKeyUp={() => setMixerControl("mixer_set_crossfader", { value: mixer.crossfader })} disabled={mixer.deckA.loadedTrackId === null && mixer.deckB.loadedTrackId === null} /></label>
          <div className="cue-controls" aria-label="Headphone cue controls">
            <div className="cue-buttons">
              <button type="button" className={mixer.cueA ? "cue-active" : ""} onClick={() => setMixerControl("deck_a_set_cue", { enabled: !mixer.cueA })} disabled={!mixer.cueSupported || mixer.deckA.loadedTrackId === null}>Cue A</button>
              <button type="button" className={mixer.cueB ? "cue-active" : ""} onClick={() => setMixerControl("deck_b_set_cue", { enabled: !mixer.cueB })} disabled={!mixer.cueSupported || mixer.deckB.loadedTrackId === null}>Cue B</button>
            </div>
            <label>Cue gain<input type="range" min={0} max={1} step={0.01} value={mixer.cueGain} onChange={(event) => setMixer({ ...mixer, cueGain: Number(event.target.value) })} onPointerUp={() => setMixerControl("mixer_set_cue_gain", { gain: mixer.cueGain })} onKeyUp={() => setMixerControl("mixer_set_cue_gain", { gain: mixer.cueGain })} disabled={!mixer.cueSupported} /></label>
            <label>Cue / master<input type="range" min={-1} max={1} step={0.01} value={mixer.cueBlend} onChange={(event) => setMixer({ ...mixer, cueBlend: Number(event.target.value) })} onPointerUp={() => setMixerControl("mixer_set_cue_blend", { value: mixer.cueBlend })} onKeyUp={() => setMixerControl("mixer_set_cue_blend", { value: mixer.cueBlend })} disabled={!mixer.cueSupported} /></label>
            <small className={mixer.cueSupported ? "routing-ready" : "routing-limitation"}>{mixer.cueSupported ? "Master: channels 1–2 · Cue: channels 3–4" : mixer.routingLimitation}</small>
          </div>
          <small>Callbacks {mixer.callbacks}<br />Clipped {mixer.clippedSamples}<br />Errors {mixer.streamErrors}</small>
        </div>
        <DeckPanel id="b" snapshot={mixer.deckB} seek={seekB} busy={busyDeck === "b"} message={messageB} onSeekChange={setSeekB} onCommand={(command, args, success) => runDeckCommand("b", command, args, success)} />
      </section>
      <section className="library-panel" aria-labelledby="library-title">
        <div className="library-header"><div><p className="eyebrow">Local files</p><h2 id="library-title">Music library</h2></div><button className="primary-button" type="button" onClick={addMusicFolder} disabled={isScanning}>{isScanning ? "Scanning..." : "Add music folder"}</button></div>
        <p className="library-status" aria-live="polite">{libraryStatus}</p>
        <div className="track-table-wrap"><table className="track-table"><thead><tr><th>Title</th><th>Artist</th><th>Format</th><th>Time</th><th>Status</th><th>Deck</th></tr></thead><tbody>
          {tracks.length === 0 ? <tr><td colSpan={6} className="empty-library">Select a folder containing MP3, WAV, FLAC, AAC/M4A, or AIFF files.</td></tr> : tracks.map((track) => <tr key={track.id} title={track.path}><td>{track.title}</td><td>{track.artist ?? "Unknown artist"}</td><td>{track.codec?.toUpperCase() ?? "Unknown"}</td><td>{formatDuration(track.durationSeconds)}</td><td><span className={track.missing ? "track-missing" : "track-ready"}>{track.missing ? "Missing" : "Ready"}</span></td><td><div className="table-actions"><button className="table-action" type="button" onClick={() => runDeckCommand("a", "deck_a_load", { trackId: track.id }, `Loaded ${track.title} into deck A.`)} disabled={track.missing || busyDeck !== null}>{mixer.deckA.loadedTrackId === track.id ? "Loaded A" : "Load A"}</button><button className="table-action" type="button" onClick={() => runDeckCommand("b", "deck_b_load", { trackId: track.id }, `Loaded ${track.title} into deck B.`)} disabled={track.missing || busyDeck !== null}>{mixer.deckB.loadedTrackId === track.id ? "Loaded B" : "Load B"}</button></div></td></tr>)}
        </tbody></table></div>
      </section>
    </main>
  );
}

export default App;
