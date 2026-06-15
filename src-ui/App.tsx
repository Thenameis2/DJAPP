import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";

type Theme = "dark" | "light";
type DeckId = "a" | "b";
type Track = { id: number; path: string; title: string; artist: string | null; durationSeconds: number | null; codec: string | null; missing: boolean };
type ScanResult = { discovered: number; inserted: number; modified: number; restored: number; missing: number; metadataFailures: number; traversalErrors: string[] };
type DeckSnapshot = { loadedTrackId: number | null; title: string | null; durationSeconds: number | null; positionSeconds: number; state: "paused" | "playing" | "ended"; underflowCallbacks: number; streamErrors: number; workerError: string | null; tempoPercent: number; keyLock: boolean; pitchSemitones: number; tempoRatio: number; processorLatencyMs: number };
type MixerSnapshot = { deckA: DeckSnapshot; deckB: DeckSnapshot; crossfader: number; channelGainA: number; channelGainB: number; masterGain: number; callbacks: number; clippedSamples: number; streamErrors: number; outputDeviceId: string | null; outputDeviceName: string | null; deviceMessage: string | null; cueA: boolean; cueB: boolean; cueBlend: number; cueGain: number; cueSupported: boolean; routingMode: string; routingPreference: string; routingLimitation: string | null; cueOutputDeviceId: string | null; cueOutputDeviceName: string | null; cueDelayMs: number; cueQueueDepthFrames: number; cueMinQueueDepthFrames: number; cueMaxQueueDepthFrames: number; cueUnderflowCallbacks: number; cueOverflowCallbacks: number; cueStreamErrors: number; cueSignalPeak: number };
type OutputDevice = { id: string; name: string; isDefault: boolean; maxChannels: number; sampleRate: number; stereoMasterSupported: boolean };

const unloadedDeck: DeckSnapshot = { loadedTrackId: null, title: null, durationSeconds: null, positionSeconds: 0, state: "paused", underflowCallbacks: 0, streamErrors: 0, workerError: null, tempoPercent: 0, keyLock: true, pitchSemitones: 0, tempoRatio: 1, processorLatencyMs: 0 };
const unloadedMixer: MixerSnapshot = { deckA: unloadedDeck, deckB: unloadedDeck, crossfader: 0, channelGainA: 1, channelGainB: 1, masterGain: 1, callbacks: 0, clippedSamples: 0, streamErrors: 0, outputDeviceId: null, outputDeviceName: null, deviceMessage: null, cueA: false, cueB: false, cueBlend: -1, cueGain: 0.5, cueSupported: false, routingMode: "master-only", routingPreference: "automatic", routingLimitation: "Load a track to detect cue capability.", cueOutputDeviceId: null, cueOutputDeviceName: null, cueDelayMs: 0, cueQueueDepthFrames: 0, cueMinQueueDepthFrames: 0, cueMaxQueueDepthFrames: 0, cueUnderflowCallbacks: 0, cueOverflowCallbacks: 0, cueStreamErrors: 0, cueSignalPeak: 0 };

const formatDuration = (seconds: number | null, remaining = false) => {
  if (seconds === null || !Number.isFinite(seconds)) return "--:--";
  const whole = Math.max(0, Math.round(seconds));
  return `${remaining ? "-" : ""}${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}`;
};

function Waveform({ id, progress }: { id: DeckId; progress: number }) {
  return <div className={`waveform waveform-${id}`}><div className="waveform-played" style={{ width: `${progress * 100}%` }} />{Array.from({ length: 72 }, (_, index) => <i key={index} style={{ height: `${18 + ((index * 17 + (id === "a" ? 11 : 29)) % 74)}%` }} />)}<span className="playhead" style={{ left: `${progress * 100}%` }} /></div>;
}

type DeckPanelProps = { id: DeckId; snapshot: DeckSnapshot; seek: number; cued: boolean; busy: boolean; message: string; onSeekChange: (value: number) => void; onCommand: (command: string, args?: Record<string, unknown>, success?: string) => Promise<void> };

function DeckPanel({ id, snapshot, seek, cued, busy, message, onSeekChange, onCommand }: DeckPanelProps) {
  const label = id.toUpperCase();
  const prefix = `deck_${id}`;
  const duration = Math.max(snapshot.durationSeconds ?? 0, 1);
  const progress = Math.min(snapshot.positionSeconds / duration, 1);
  const [tempoDraft, setTempoDraft] = useState(snapshot.tempoPercent);
  const [editingTempo, setEditingTempo] = useState(false);
  useEffect(() => { if (!editingTempo) setTempoDraft(snapshot.tempoPercent); }, [snapshot.tempoPercent, editingTempo]);
  const commitTempo = async (value: number) => { setTempoDraft(value); await onCommand(`${prefix}_set_tempo`, { percent: value }); setEditingTempo(false); };
  const commitSeek = () => onCommand(`${prefix}_seek`, { seconds: seek, resume: snapshot.state === "playing" });
  return <article className={`deck deck-${id}`}>
    <header className="track-strip">
      <div className="track-art">♪</div>
      <div className="track-copy"><strong>{snapshot.title ?? `Load a track into Deck ${label}`}</strong><span>{snapshot.loadedTrackId ? `Deck ${label} · local audio` : "Waiting for music"}</span></div>
      <time>{formatDuration(Math.max((snapshot.durationSeconds ?? 0) - snapshot.positionSeconds, 0), true)}</time>
    </header>
    <Waveform id={id} progress={progress} />
    <input className="waveform-seek" type="range" min={0} max={duration} step={0.1} value={Math.min(seek, duration)} onChange={(event) => onSeekChange(Number(event.target.value))} onPointerUp={commitSeek} onKeyUp={commitSeek} disabled={snapshot.loadedTrackId === null || busy} aria-label={`Deck ${label} position`} />
    <div className="deck-body">
      <aside className="tempo-rail"><button type="button" disabled title="Sync requires BPM and beat-grid analysis">SYNC</button><strong title="Track BPM analysis is not implemented yet">VINYL RATE</strong><span>{(100 * snapshot.tempoRatio).toFixed(1)}%</span><input type="range" min={-16} max={16} step={0.1} value={tempoDraft} onPointerDown={() => setEditingTempo(true)} onChange={(event) => setTempoDraft(Number(event.target.value))} onPointerUp={(event) => void commitTempo(Number(event.currentTarget.value))} onKeyUp={(event) => void commitTempo(Number(event.currentTarget.value))} disabled={!snapshot.loadedTrackId || busy} title="Reliable varispeed: pitch changes with playback speed" aria-label={`Deck ${label} vinyl playback rate`} /><small>KEY LOCK OFF</small><button type="button" disabled title="Key lock is unavailable while spectral processing is under review">KEY</button><label className="pitch-control">PITCH LOCKED<input type="range" min={-12} max={12} step={0.1} value={0} disabled title="Independent pitch is unavailable while spectral processing is under review" aria-label={`Deck ${label} pitch shift unavailable`} /></label></aside>
      <div className={`platter ${snapshot.state === "playing" ? "platter-playing" : ""}`}><div className="vinyl-rings"><span>DECK {label}</span></div><div className="tonearm" /></div>
      <div className="channel-strip"><span>FILTER</span><div className="knob" /><span>HIGH</span><div className="knob" /><span>MID</span><div className="knob" /><span>LOW</span><div className="knob" /></div>
    </div>
    <footer className="transport-row">
      <button type="button" className={cued ? "cue-lit" : ""} onClick={() => onCommand(`${prefix}_set_cue`, { enabled: !cued })} disabled={!snapshot.loadedTrackId}>CUE {cued ? "ON" : ""}</button>
      <button type="button" onClick={() => onCommand(`${prefix}_stop`)} disabled={!snapshot.loadedTrackId}>RESET</button>
      <span className={`deck-state state-${snapshot.state}`}>{snapshot.state}</span>
      <button type="button" className="play-button" onClick={() => onCommand(snapshot.state === "playing" ? `${prefix}_pause` : `${prefix}_play`)} disabled={!snapshot.loadedTrackId || busy}>{snapshot.state === "playing" ? "Ⅱ" : "▶"}</button>
    </footer>
    <small className="deck-feedback">{snapshot.workerError ?? message} · Stretch {snapshot.processorLatencyMs.toFixed(0)} ms · Underflows {snapshot.underflowCallbacks}</small>
  </article>;
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
  const [messageA, setMessageA] = useState("Choose a library track for Deck A.");
  const [messageB, setMessageB] = useState("Choose a library track for Deck B.");
  const [busyDeck, setBusyDeck] = useState<DeckId | null>(null);
  const [outputDevices, setOutputDevices] = useState<OutputDevice[]>([]);
  const [deviceStatus, setDeviceStatus] = useState("Discovering audio outputs...");
  const [isSwitchingDevice, setIsSwitchingDevice] = useState(false);

  useEffect(() => { document.documentElement.dataset.theme = theme; }, [theme]);
  useEffect(() => { invoke<string>("engine_status").then(setEngineStatus).catch((error: unknown) => setEngineStatus(`Engine unavailable: ${String(error)}`)); }, []);
  useEffect(() => { void refreshLibrary(); }, []);
  useEffect(() => { void refreshOutputDevices(); const timer = window.setInterval(() => void refreshOutputDevices(true), 3000); return () => window.clearInterval(timer); }, []);
  useEffect(() => { let cancelled = false; const refresh = async () => { try { const snapshot = await invoke<MixerSnapshot>("mixer_snapshot"); if (!cancelled) applySnapshot(snapshot); } catch (error: unknown) { if (!cancelled) setMessageA(`Mixer unavailable: ${String(error)}`); } }; void refresh(); const timer = window.setInterval(() => void refresh(), 500); return () => { cancelled = true; window.clearInterval(timer); }; }, []);

  const applySnapshot = (snapshot: MixerSnapshot) => { setMixer(snapshot); setSeekA(snapshot.deckA.positionSeconds); setSeekB(snapshot.deckB.positionSeconds); };
  const refreshLibrary = async () => { try { setTracks(await invoke<Track[]>("library_tracks")); } catch (error: unknown) { setLibraryStatus(`Could not load library: ${String(error)}`); } };
  const refreshOutputDevices = async (silent = false) => { try { const devices = await invoke<OutputDevice[]>("audio_output_devices"); setOutputDevices(devices); if (!silent) setDeviceStatus(`${devices.length} output device${devices.length === 1 ? "" : "s"} available.`); } catch (error: unknown) { if (!silent) setDeviceStatus(`Could not discover outputs: ${String(error)}`); } };
  const addMusicFolder = async () => { const selected = await open({ directory: true, multiple: false }); if (!selected) return; setIsScanning(true); setLibraryStatus(`Scanning ${selected}...`); try { const result = await invoke<ScanResult>("scan_music_folder", { path: selected }); setLibraryStatus(`Found ${result.discovered}: ${result.inserted} new, ${result.modified} changed, ${result.restored} restored, ${result.missing} missing, ${result.metadataFailures} metadata warnings.`); await refreshLibrary(); } catch (error: unknown) { setLibraryStatus(`Scan failed: ${String(error)}`); } finally { setIsScanning(false); } };
  const runDeckCommand = async (id: DeckId, command: string, args?: Record<string, unknown>, success?: string) => { setBusyDeck(id); try { const snapshot = await invoke<MixerSnapshot>(command, args); applySnapshot(snapshot); const text = success ?? `Deck ${id.toUpperCase()} is ${id === "a" ? snapshot.deckA.state : snapshot.deckB.state}.`; id === "a" ? setMessageA(text) : setMessageB(text); } catch (error: unknown) { id === "a" ? setMessageA(String(error)) : setMessageB(String(error)); } finally { setBusyDeck(null); } };
  const setMixerControl = async (command: string, args: Record<string, unknown>) => { try { applySnapshot(await invoke<MixerSnapshot>(command, args)); } catch (error: unknown) { setMessageA(`Mixer error: ${String(error)}`); } };
  const setAudioRouting = async (command: string, args: Record<string, unknown>, pending: string) => { setIsSwitchingDevice(true); setDeviceStatus(pending); try { const snapshot = await invoke<MixerSnapshot>(command, args); applySnapshot(snapshot); setDeviceStatus(snapshot.deviceMessage ?? "Audio routing updated."); } catch (error: unknown) { setDeviceStatus(`Audio routing failed: ${String(error)}`); } finally { setIsSwitchingDevice(false); } };

  return <main className="app-shell">
    <header className="topbar"><div className="window-lights"><i /><i /><i /></div><strong className="brand">DJ APP <span>PRO</span></strong><div className="top-actions"><span className="engine-pill">● {engineStatus}</span><button type="button" onClick={() => setTheme(theme === "dark" ? "light" : "dark")}>{theme === "dark" ? "☀" : "◐"}</button></div></header>
    <details className="audio-panel"><summary>Audio routing · {mixer.outputDeviceName ?? "select master"} · {mixer.routingMode}</summary><div className="audio-grid">
      <label>Master output<select value={mixer.outputDeviceId ?? ""} onChange={(event) => setAudioRouting("audio_select_output_device", { deviceId: event.target.value }, "Switching master output...")} disabled={isSwitchingDevice}>{mixer.outputDeviceId === null && <option value="">Select output</option>}{outputDevices.map((device) => <option key={device.id} value={device.id}>{device.name}{device.isDefault ? " · default" : ""} · {device.maxChannels}ch</option>)}</select></label>
      <label>Cue routing<select value={mixer.routingPreference} onChange={(event) => setAudioRouting("audio_set_routing_mode", { mode: event.target.value }, "Updating cue routing...")}><option value="automatic">Automatic / 4-channel</option><option value="master-only">Master only</option><option value="dual-device-cue">Separate headphones</option></select></label>
      <label>Cue output<select value={mixer.cueOutputDeviceId ?? ""} onChange={(event) => setAudioRouting("audio_select_cue_output_device", { deviceId: event.target.value }, "Selecting headphones...")}><option value="">Select output</option>{outputDevices.filter((device) => device.id !== mixer.outputDeviceId && device.stereoMasterSupported).map((device) => <option key={device.id} value={device.id}>{device.name}</option>)}</select></label>
      <label>Cue delay {mixer.cueDelayMs} ms<input type="range" min={0} max={250} value={mixer.cueDelayMs} onChange={(event) => setMixer({ ...mixer, cueDelayMs: Number(event.target.value) })} onPointerUp={() => setAudioRouting("audio_set_cue_delay_ms", { delayMs: mixer.cueDelayMs }, "Applying cue delay...")} /></label>
      <p>{mixer.deviceMessage ?? deviceStatus} {mixer.routingLimitation}</p>
    </div></details>
    <section className="deck-grid">
      <DeckPanel id="a" snapshot={mixer.deckA} seek={seekA} cued={mixer.cueA} busy={busyDeck === "a"} message={messageA} onSeekChange={setSeekA} onCommand={(command, args, success) => runDeckCommand("a", command, args, success)} />
      <section className="center-mixer"><strong>MIXER</strong><label>A<input className="vertical-fader" type="range" min={0} max={1} step={0.01} value={mixer.channelGainA} onChange={(event) => setMixer({ ...mixer, channelGainA: Number(event.target.value) })} onPointerUp={() => setMixerControl("deck_a_set_gain", { gain: mixer.channelGainA })} /></label><div className="meters"><i /><i /><i /><i /><i /><i /></div><label>B<input className="vertical-fader" type="range" min={0} max={1} step={0.01} value={mixer.channelGainB} onChange={(event) => setMixer({ ...mixer, channelGainB: Number(event.target.value) })} onPointerUp={() => setMixerControl("deck_b_set_gain", { gain: mixer.channelGainB })} /></label><label className="master-control">MASTER<input type="range" min={0} max={1} step={0.01} value={mixer.masterGain} onChange={(event) => setMixer({ ...mixer, masterGain: Number(event.target.value) })} onPointerUp={() => setMixerControl("mixer_set_master_gain", { gain: mixer.masterGain })} /></label><div className="cue-mini"><button className={mixer.cueA ? "cue-lit" : ""} onClick={() => setMixerControl("deck_a_set_cue", { enabled: !mixer.cueA })}>A</button><span>AUTO CUE</span><button className={mixer.cueB ? "cue-lit" : ""} onClick={() => setMixerControl("deck_b_set_cue", { enabled: !mixer.cueB })}>B</button></div><div className="headphone-controls"><label>CUE LEVEL {Math.round(mixer.cueGain * 100)}%<input type="range" min={0} max={1} step={0.01} value={mixer.cueGain} onChange={(event) => setMixer({ ...mixer, cueGain: Number(event.target.value) })} onPointerUp={() => setMixerControl("mixer_set_cue_gain", { gain: mixer.cueGain })} /></label><label>CUE / MASTER<input type="range" min={-1} max={1} step={0.01} value={mixer.cueBlend} onChange={(event) => setMixer({ ...mixer, cueBlend: Number(event.target.value) })} onPointerUp={() => setMixerControl("mixer_set_cue_blend", { value: mixer.cueBlend })} /></label><small className={mixer.cueSignalPeak > 0.0001 ? "signal-live" : "signal-silent"}>{mixer.cueSignalPeak > 0.0001 ? `Cue signal ${Math.round(mixer.cueSignalPeak * 100)}%` : "Cue signal silent"}</small></div></section>
      <DeckPanel id="b" snapshot={mixer.deckB} seek={seekB} cued={mixer.cueB} busy={busyDeck === "b"} message={messageB} onSeekChange={setSeekB} onCommand={(command, args, success) => runDeckCommand("b", command, args, success)} />
    </section>
    <section className="crossfader-dock"><span>A</span><input type="range" min={-1} max={1} step={0.01} value={mixer.crossfader} onChange={(event) => setMixer({ ...mixer, crossfader: Number(event.target.value) })} onPointerUp={() => setMixerControl("mixer_set_crossfader", { value: mixer.crossfader })} onKeyUp={() => setMixerControl("mixer_set_crossfader", { value: mixer.crossfader })} /><span>B</span><small>{Math.abs(mixer.crossfader) <= 0.05 ? "Both decks on master · auto cue off" : mixer.crossfader < 0 ? "Deck B automatically cued" : "Deck A automatically cued"}</small></section>
    <section className="browser"><aside className="library-sidebar"><strong>MY FILES</strong><button className="add-folder" onClick={addMusicFolder} disabled={isScanning}>＋ Add folder</button><span>Local Music</span><span>Recently Added</span><span>Missing Files</span></aside><div className="library-main"><header><div><strong>Music Library</strong><small>{libraryStatus}</small></div><button onClick={addMusicFolder} disabled={isScanning}>{isScanning ? "Scanning..." : "Scan folder"}</button></header><div className="track-table-wrap"><table className="track-table"><thead><tr><th>#</th><th>Title</th><th>Artist</th><th>Format</th><th>Time</th><th>Load</th></tr></thead><tbody>{tracks.length === 0 ? <tr><td colSpan={6} className="empty-library">Add a folder containing MP3, WAV, FLAC, AAC/M4A, or AIFF files.</td></tr> : tracks.map((track, index) => <tr key={track.id} className={track.missing ? "missing-row" : ""}><td>{index + 1}</td><td><strong>{track.title}</strong></td><td>{track.artist ?? "Unknown artist"}</td><td>{track.codec?.toUpperCase() ?? "--"}</td><td>{formatDuration(track.durationSeconds)}</td><td><div className="table-actions"><button onClick={() => runDeckCommand("a", "deck_a_load", { trackId: track.id }, `Loaded ${track.title} into Deck A.`)} disabled={track.missing || busyDeck !== null}>A</button><button onClick={() => runDeckCommand("b", "deck_b_load", { trackId: track.id }, `Loaded ${track.title} into Deck B.`)} disabled={track.missing || busyDeck !== null}>B</button></div></td></tr>)}</tbody></table></div></div><aside className="automix-panel"><strong>AUTOMIX</strong><button disabled>Start AutoMix</button><p>Queue and intelligent transitions are planned for the AutoMix milestone.</p><small>Audio health<br />Callbacks {mixer.callbacks}<br />Clipped {mixer.clippedSamples}<br />Errors {mixer.streamErrors}<br />Cue queue {mixer.cueQueueDepthFrames}<br />Cue underflows {mixer.cueUnderflowCallbacks}<br />Cue errors {mixer.cueStreamErrors}</small></aside></section>
  </main>;
}

export default App;
