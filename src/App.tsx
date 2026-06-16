import { listen } from "@tauri-apps/api/event";
import {
  EyeOff,
  Keyboard,
  Plus,
  Power,
  RotateCcw,
  Save,
  SlidersHorizontal,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import type { AppConfig, ApplyResult, ChannelMode, ChannelSettings, DisplayInfo, Profile } from "./types";

const emptyChannel: ChannelSettings = {
  gamma: 1,
  brightness: 0,
  contrast: 1,
};

function newProfile(displayId = ""): Profile {
  return {
    id: "",
    name: "New profile",
    targetDisplayId: displayId,
    channelMode: "linked",
    linked: { ...emptyChannel },
    red: { ...emptyChannel },
    green: { ...emptyChannel },
    blue: { ...emptyChannel },
    hotkey: null,
  };
}

function profileFromConfig(config: AppConfig, fallbackDisplayId: string): Profile {
  const selected =
    config.profiles.find((profile) => profile.id === config.selectedProfileId) ??
    config.profiles[0];
  return selected ? { ...selected } : newProfile(fallbackDisplayId);
}

export default function App() {
  const [displays, setDisplays] = useState<DisplayInfo[]>([]);
  const [config, setConfig] = useState<AppConfig>({ version: 1, profiles: [], selectedProfileId: null });
  const [draft, setDraft] = useState<Profile>(newProfile());
  const [status, setStatus] = useState("Loading");
  const [error, setError] = useState<string | null>(null);
  const [isBusy, setIsBusy] = useState(false);
  const [dirtyDraft, setDirtyDraft] = useState(false);
  const bootedRef = useRef(false);

  const selectedDisplay = useMemo(
    () => displays.find((display) => display.id === draft.targetDisplayId) ?? null,
    [displays, draft.targetDisplayId],
  );

  const channel = draft.channelMode === "linked" ? draft.linked : draft.red;

  useEffect(() => {
    let cancelled = false;

    async function load() {
      try {
        const [displayList, appConfig] = await Promise.all([api.listDisplays(), api.loadConfig()]);
        if (cancelled) {
          return;
        }

        const fallbackDisplayId = displayList[0]?.id ?? "";
        setDisplays(displayList);
        setConfig(appConfig);
        setDraft(profileFromConfig(appConfig, fallbackDisplayId));
        setStatus("Ready");
        bootedRef.current = true;
      } catch (caught) {
        setError(String(caught));
        setStatus("Load failed");
      }
    }

    load();

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const unlisten = Promise.all([
      listen<ApplyResult>("profile-applied", (event) => {
        setStatus(`Applied ${event.payload.displayId}`);
        setError(null);
      }),
      listen<string>("profile-apply-error", (event) => {
        setError(event.payload);
      }),
    ]);

    return () => {
      unlisten.then((callbacks) => callbacks.forEach((callback) => callback()));
    };
  }, []);

  useEffect(() => {
    if (!bootedRef.current || !dirtyDraft || !selectedDisplay?.isSupported) {
      return;
    }

    const timeout = window.setTimeout(async () => {
      try {
        await api.applyDraftProfile(draft);
        setStatus("Preview applied");
        setError(null);
      } catch (caught) {
        setError(String(caught));
      } finally {
        setDirtyDraft(false);
      }
    }, 260);

    return () => window.clearTimeout(timeout);
  }, [dirtyDraft, draft, selectedDisplay?.isSupported]);

  function updateDraft(updater: (profile: Profile) => Profile, shouldApply = true) {
    setDraft((current) => updater({ ...current }));
    if (shouldApply) {
      setDirtyDraft(true);
    }
  }

  function updateChannel(key: keyof ChannelSettings, value: number, channelName?: "red" | "green" | "blue") {
    updateDraft((profile) => {
      if (profile.channelMode === "linked" || !channelName) {
        return { ...profile, linked: { ...profile.linked, [key]: value } };
      }

      return {
        ...profile,
        [channelName]: {
          ...profile[channelName],
          [key]: value,
        },
      };
    });
  }

  async function saveCurrentProfile() {
    setIsBusy(true);
    try {
      const saved = await api.saveProfile(draft);
      const fallbackDisplayId = displays[0]?.id ?? "";
      setConfig(saved);
      setDraft(profileFromConfig(saved, fallbackDisplayId));
      setStatus("Saved");
      setError(null);
      setDirtyDraft(false);
    } catch (caught) {
      setError(String(caught));
    } finally {
      setIsBusy(false);
    }
  }

  async function deleteCurrentProfile() {
    if (!draft.id) {
      setDraft(newProfile(displays[0]?.id ?? ""));
      return;
    }

    setIsBusy(true);
    try {
      const saved = await api.deleteProfile(draft.id);
      const fallbackDisplayId = displays[0]?.id ?? "";
      setConfig(saved);
      setDraft(profileFromConfig(saved, fallbackDisplayId));
      setStatus("Deleted");
      setError(null);
      setDirtyDraft(false);
    } catch (caught) {
      setError(String(caught));
    } finally {
      setIsBusy(false);
    }
  }

  async function applyCurrentProfile() {
    setIsBusy(true);
    try {
      if (draft.id) {
        await api.applyProfile(draft.id);
      } else {
        await api.applyDraftProfile(draft);
      }
      setStatus("Applied");
      setError(null);
      setDirtyDraft(false);
    } catch (caught) {
      setError(String(caught));
    } finally {
      setIsBusy(false);
    }
  }

  async function reset(linear: boolean) {
    setIsBusy(true);
    try {
      await api.resetDisplay(draft.targetDisplayId, linear);
      setStatus(linear ? "Linear LUT applied" : "Startup ramp restored");
      setError(null);
    } catch (caught) {
      setError(String(caught));
    } finally {
      setIsBusy(false);
    }
  }

  return (
    <main className="app-shell">
      <header className="topbar">
        <div>
          <h1>GammaDeck</h1>
          <p>{status}</p>
        </div>
        <div className="topbar-actions">
          <button className="icon-button" onClick={() => api.hideWindow()} title="Hide">
            <EyeOff size={18} />
          </button>
          <button className="icon-button danger" onClick={() => api.exitApp()} title="Exit">
            <Power size={18} />
          </button>
        </div>
      </header>

      {error ? <div className="error-banner">{error}</div> : null}

      <section className="control-band">
        <label className="field">
          <span>Display</span>
          <select
            value={draft.targetDisplayId}
            onChange={(event) =>
              updateDraft((profile) => ({ ...profile, targetDisplayId: event.target.value }))
            }
          >
            {displays.map((display) => (
              <option key={display.id} value={display.id}>
                {display.name}
                {display.isPrimary ? " - Primary" : ""}
                {display.isSupported ? "" : " - Unsupported"}
              </option>
            ))}
          </select>
        </label>

        <div className="mode-row">
          <button
            className={draft.channelMode === "linked" ? "segmented active" : "segmented"}
            onClick={() => updateDraft((profile) => ({ ...profile, channelMode: "linked" }))}
          >
            Linked
          </button>
          <button
            className={draft.channelMode === "rgb" ? "segmented active" : "segmented"}
            onClick={() => updateDraft((profile) => ({ ...profile, channelMode: "rgb" }))}
          >
            RGB
          </button>
        </div>

        <LutPreview profile={draft} />

        {draft.channelMode === "linked" ? (
          <SliderGroup settings={channel} onChange={updateChannel} />
        ) : (
          <div className="rgb-grid">
            <ChannelColumn name="red" label="Red" settings={draft.red} onChange={updateChannel} />
            <ChannelColumn name="green" label="Green" settings={draft.green} onChange={updateChannel} />
            <ChannelColumn name="blue" label="Blue" settings={draft.blue} onChange={updateChannel} />
          </div>
        )}
      </section>

      <section className="profile-band">
        <label className="field">
          <span>Profiles</span>
          <select
            value={draft.id}
            onChange={(event) => {
              const next = config.profiles.find((profile) => profile.id === event.target.value);
              if (next) {
                setDraft({ ...next });
                setDirtyDraft(false);
              }
            }}
          >
            <option value="">Unsaved profile</option>
            {config.profiles.map((profile) => (
              <option key={profile.id} value={profile.id}>
                {profile.name}
              </option>
            ))}
          </select>
        </label>

        <label className="field">
          <span>Name</span>
          <input
            value={draft.name}
            onChange={(event) =>
              updateDraft((profile) => ({ ...profile, name: event.target.value }), false)
            }
          />
        </label>

        <label className="field hotkey-field">
          <span>
            <Keyboard size={14} />
            Hot-key
          </span>
          <input
            value={draft.hotkey ?? ""}
            placeholder="Control+Alt+Digit1"
            onChange={(event) =>
              updateDraft((profile) => ({ ...profile, hotkey: event.target.value || null }), false)
            }
            onKeyDown={(event) => {
              const binding = formatShortcut(event);
              if (!binding) {
                return;
              }
              event.preventDefault();
              updateDraft((profile) => ({ ...profile, hotkey: binding }), false);
            }}
          />
        </label>

        <div className="button-grid">
          <button onClick={() => setDraft(newProfile(displays[0]?.id ?? ""))}>
            <Plus size={16} />
            New
          </button>
          <button onClick={saveCurrentProfile} disabled={isBusy}>
            <Save size={16} />
            Save as
          </button>
          <button onClick={deleteCurrentProfile} disabled={isBusy}>
            <Trash2 size={16} />
            Delete
          </button>
          <button onClick={applyCurrentProfile} disabled={isBusy || !selectedDisplay?.isSupported}>
            <SlidersHorizontal size={16} />
            Apply
          </button>
          <button onClick={() => reset(false)} disabled={isBusy || !draft.targetDisplayId}>
            <RotateCcw size={16} />
            Reset
          </button>
          <button onClick={() => reset(true)} disabled={isBusy || !draft.targetDisplayId}>
            Linear
          </button>
        </div>
      </section>
    </main>
  );
}

function SliderGroup({
  settings,
  onChange,
}: {
  settings: ChannelSettings;
  onChange: (key: keyof ChannelSettings, value: number) => void;
}) {
  return (
    <div className="slider-stack">
      <RangeRow label="Gamma" min={0.5} max={2.5} step={0.01} value={settings.gamma} onChange={(value) => onChange("gamma", value)} />
      <RangeRow label="Brightness" min={-0.35} max={0.35} step={0.01} value={settings.brightness} onChange={(value) => onChange("brightness", value)} />
      <RangeRow label="Contrast" min={0.5} max={1.75} step={0.01} value={settings.contrast} onChange={(value) => onChange("contrast", value)} />
    </div>
  );
}

function ChannelColumn({
  name,
  label,
  settings,
  onChange,
}: {
  name: "red" | "green" | "blue";
  label: string;
  settings: ChannelSettings;
  onChange: (key: keyof ChannelSettings, value: number, channelName?: "red" | "green" | "blue") => void;
}) {
  return (
    <div className={`channel-column ${name}`}>
      <h2>{label}</h2>
      <RangeRow label="G" min={0.5} max={2.5} step={0.01} value={settings.gamma} onChange={(value) => onChange("gamma", value, name)} />
      <RangeRow label="B" min={-0.35} max={0.35} step={0.01} value={settings.brightness} onChange={(value) => onChange("brightness", value, name)} />
      <RangeRow label="C" min={0.5} max={1.75} step={0.01} value={settings.contrast} onChange={(value) => onChange("contrast", value, name)} />
    </div>
  );
}

function RangeRow({
  label,
  min,
  max,
  step,
  value,
  onChange,
}: {
  label: string;
  min: number;
  max: number;
  step: number;
  value: number;
  onChange: (value: number) => void;
}) {
  return (
    <label className="range-row">
      <span>{label}</span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
      <input
        className="number-input"
        type="number"
        min={min}
        max={max}
        step={step}
        value={value.toFixed(2)}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </label>
  );
}

function LutPreview({ profile }: { profile: Profile }) {
  const channels =
    profile.channelMode === "linked"
      ? [{ className: "linked", settings: profile.linked }]
      : [
          { className: "red", settings: profile.red },
          { className: "green", settings: profile.green },
          { className: "blue", settings: profile.blue },
        ];

  return (
    <svg className="lut-preview" viewBox="0 0 320 150" role="img" aria-label="LUT curve preview">
      <path className="grid-line" d="M20 130 H300 M20 95 H300 M20 60 H300 M20 25 H300" />
      <path className="grid-line" d="M20 20 V130 M90 20 V130 M160 20 V130 M230 20 V130 M300 20 V130" />
      {channels.map((channel) => (
        <path key={channel.className} className={`curve ${channel.className}`} d={curvePath(channel.settings)} />
      ))}
    </svg>
  );
}

function curvePath(settings: ChannelSettings) {
  const points = Array.from({ length: 32 }, (_, index) => {
    const x = index / 31;
    const gamma = Math.min(2.5, Math.max(0.5, settings.gamma));
    const brightness = Math.min(0.35, Math.max(-0.35, settings.brightness));
    const contrast = Math.min(1.75, Math.max(0.5, settings.contrast));
    const y = Math.min(0.98, Math.max(0.02, (Math.pow(x, 1 / gamma) - 0.5) * contrast + 0.5 + brightness));
    return [20 + x * 280, 130 - y * 110];
  });

  return points.map(([x, y], index) => `${index === 0 ? "M" : "L"} ${x.toFixed(2)} ${y.toFixed(2)}`).join(" ");
}

function formatShortcut(event: React.KeyboardEvent<HTMLInputElement>) {
  const ignored = new Set(["Control", "Shift", "Alt", "Meta", "Tab", "Escape"]);
  if (ignored.has(event.key)) {
    return "";
  }

  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Control");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Super");
  parts.push(event.code);
  return parts.join("+");
}
