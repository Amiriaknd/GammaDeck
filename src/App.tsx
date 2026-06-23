import { ChevronDown, Plus, RotateCcw, Save } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import type { AppConfig, ApplyResult, ChannelSettings, DisplayInfo, Profile } from "./types";

const DEFAULT_PROFILE_ID = "default";
type BaselineDialogTarget = "current" | "initial" | "neutral";

const emptyChannel: ChannelSettings = {
  gamma: 1,
  brightness: 0,
  contrast: 1,
};

function newProfile(displayId = "", name = "New profile"): Profile {
  return {
    id: "",
    name,
    targetDisplayId: displayId,
    channelMode: "linked",
    linked: { ...emptyChannel },
    red: { ...emptyChannel },
    green: { ...emptyChannel },
    blue: { ...emptyChannel },
    hotkey: null,
  };
}

function defaultProfile(displayId = ""): Profile {
  return {
    ...newProfile(displayId, "Default"),
    id: DEFAULT_PROFILE_ID,
  };
}

function neutralizeProfile(profile: Profile): Profile {
  return {
    ...profile,
    channelMode: "linked",
    linked: { ...emptyChannel },
    red: { ...emptyChannel },
    green: { ...emptyChannel },
    blue: { ...emptyChannel },
  };
}

function profileFromConfig(config: AppConfig, fallbackDisplayId: string): Profile {
  const profiles = sortedProfiles(config.profiles);
  const selected =
    profiles.find((profile) => profile.id === config.selectedProfileId) ??
    profiles.find((profile) => profile.id === DEFAULT_PROFILE_ID) ??
    profiles[0];
  return selected ? cloneProfile(selected) : defaultProfile(fallbackDisplayId);
}

function cloneProfile(profile: Profile): Profile {
  return {
    ...profile,
    linked: { ...profile.linked },
    red: { ...profile.red },
    green: { ...profile.green },
    blue: { ...profile.blue },
  };
}

function sortedProfiles(profiles: Profile[]) {
  return [...profiles].sort((left, right) => {
    if (left.id === DEFAULT_PROFILE_ID) return -1;
    if (right.id === DEFAULT_PROFILE_ID) return 1;
    return left.name.localeCompare(right.name);
  });
}

export default function App() {
  const [displays, setDisplays] = useState<DisplayInfo[]>([]);
  const [config, setConfig] = useState<AppConfig>({
    version: 2,
    initialDisplayBaselines: [],
    displayBaselines: [],
    profiles: [],
    selectedProfileId: null,
  });
  const [draft, setDraft] = useState<Profile>(defaultProfile());
  const [status, setStatus] = useState("Loading");
  const [error, setError] = useState<string | null>(null);
  const [isBusy, setIsBusy] = useState(false);
  const [dirtyDraft, setDirtyDraft] = useState(false);
  const [editingProfileId, setEditingProfileId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState("");
  const [baselineDialogTarget, setBaselineDialogTarget] = useState<BaselineDialogTarget | null>(null);
  const [isBaselineMenuOpen, setIsBaselineMenuOpen] = useState(false);
  const bootedRef = useRef(false);
  const autosaveRevisionRef = useRef(0);
  const configRef = useRef(config);

  const selectedDisplay = useMemo(
    () => displays.find((display) => display.id === draft.targetDisplayId) ?? null,
    [displays, draft.targetDisplayId],
  );

  const profiles = useMemo(() => sortedProfiles(config.profiles), [config.profiles]);
  const settings = draft.channelMode === "linked" ? draft.linked : draft.red;
  const visibleError = error && !isUnsupportedGammaError(error) ? error : null;

  useEffect(() => {
    configRef.current = config;
  }, [config]);

  useEffect(() => {
    let cancelled = false;

    async function load() {
      try {
        const [displayList, appConfig] = await Promise.all([api.listDisplays(), api.loadConfig()]);
        if (cancelled) {
          return;
        }

        const fallbackDisplayId = displayList[0]?.id ?? "";
        let nextConfig = appConfig;

        if (!appConfig.profiles.some((profile) => profile.id === DEFAULT_PROFILE_ID)) {
          nextConfig = await api.saveProfile(defaultProfile(fallbackDisplayId));
        }

        if (cancelled) {
          return;
        }

        setDisplays(displayList);
        setConfig(nextConfig);
        setDraft(profileFromConfig(nextConfig, fallbackDisplayId));
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
    const onHotkey = (event: Event) => {
      const profileId = (event as CustomEvent<string>).detail;
      devLog("dom profile-hotkey", profileId);
      syncAppliedProfile(profileId);
    };

    const onApplied = (event: Event) => {
      const result = (event as CustomEvent<ApplyResult>).detail;
      devLog("dom profile-applied", result);
      syncAppliedProfile(result.profileId);
      setStatus(`Applied ${result.displayId}`);
      setError(null);
    };

    const onApplyError = (event: Event) => {
      const message = (event as CustomEvent<string>).detail;
      devLog("dom profile-apply-error", message);
      setError(message);
    };

    window.addEventListener("gammadeck-profile-hotkey", onHotkey);
    window.addEventListener("gammadeck-profile-applied", onApplied);
    window.addEventListener("gammadeck-profile-apply-error", onApplyError);
    devLog("dom listeners registered");

    return () => {
      window.removeEventListener("gammadeck-profile-hotkey", onHotkey);
      window.removeEventListener("gammadeck-profile-applied", onApplied);
      window.removeEventListener("gammadeck-profile-apply-error", onApplyError);
    };
  }, []);

  useEffect(() => {
    if (!bootedRef.current || !dirtyDraft || !draft.id) {
      return;
    }

    const profile = cloneProfile(draft);
    const revision = ++autosaveRevisionRef.current;
    let cancelled = false;
    const isCurrentAutosave = () => !cancelled && revision === autosaveRevisionRef.current;

    const timeout = window.setTimeout(async () => {
      try {
        const saved = await api.saveProfile(profile);
        if (!isCurrentAutosave()) {
          return;
        }

        setConfig(saved);
        setStatus("Auto-saved");

        if (selectedDisplay?.isSupported) {
          await api.applyDraftProfile(profile);
          if (!isCurrentAutosave()) {
            return;
          }

          setStatus("Auto-saved · Preview applied");
        }

        setError(null);
        setDirtyDraft(false);
      } catch (caught) {
        if (isCurrentAutosave()) {
          setError(String(caught));
        }
      }
    }, 280);

    return () => {
      cancelled = true;
      window.clearTimeout(timeout);
    };
  }, [dirtyDraft, draft, selectedDisplay?.isSupported]);

  function updateDraft(updater: (profile: Profile) => Profile) {
    setDraft((current) => updater(cloneProfile(current)));
    setDirtyDraft(true);
  }

  function syncAppliedProfile(profileId: string | null) {
    devLog("syncAppliedProfile", profileId);
    if (!profileId) {
      return;
    }

    autosaveRevisionRef.current += 1;
    setDirtyDraft(false);
    setEditingProfileId(null);

    const localProfile = configRef.current.profiles.find((profile) => profile.id === profileId);
    if (localProfile) {
      devLog("syncAppliedProfile local match", { id: localProfile.id, name: localProfile.name });
      setDraft(cloneProfile(localProfile));
      setConfig((current) =>
        current.selectedProfileId === profileId ? current : { ...current, selectedProfileId: profileId },
      );
      return;
    }

    devLog("syncAppliedProfile local miss", profileId);
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

  async function selectProfile(profile: Profile) {
    setDraft(cloneProfile(profile));
    setDirtyDraft(false);
    setEditingProfileId(null);

    setIsBusy(true);
    try {
      await api.applyProfile(profile.id);
      setStatus("Applied");
      setError(null);
    } catch (caught) {
      setError(String(caught));
    } finally {
      setIsBusy(false);
    }
  }

  async function createProfile() {
    const baseName = `Profile ${profiles.filter((profile) => profile.id !== DEFAULT_PROFILE_ID).length + 1}`;
    const profile = newProfile(displays[0]?.id ?? "", baseName);

    setIsBusy(true);
    try {
      const saved = await api.saveProfile(profile);
      const next = profileFromConfig(saved, displays[0]?.id ?? "");
      setConfig(saved);
      setDraft(next);
      setEditingProfileId(next.id);
      setEditingName(next.name);
      setStatus("Created");
      setError(null);
      setDirtyDraft(false);
    } catch (caught) {
      setError(String(caught));
    } finally {
      setIsBusy(false);
    }
  }

  async function deleteProfile(profileId: string) {
    if (profileId === DEFAULT_PROFILE_ID) {
      return;
    }

    setIsBusy(true);
    try {
      const saved = await api.deleteProfile(profileId);
      const fallbackDisplayId = displays[0]?.id ?? "";
      setConfig(saved);

      if (draft.id === profileId) {
        setDraft(profileFromConfig(saved, fallbackDisplayId));
      }

      setStatus("Deleted");
      setError(null);
      setDirtyDraft(false);
      setEditingProfileId(null);
    } catch (caught) {
      setError(String(caught));
    } finally {
      setIsBusy(false);
    }
  }

  function resetProfile() {
    updateDraft((profile) => neutralizeProfile(profile));
    setStatus("Profile reset");
  }

  async function updateBaseline(target: BaselineDialogTarget) {
    if (!selectedDisplay?.isSupported) {
      return;
    }

    autosaveRevisionRef.current += 1;
    setIsBusy(true);
    try {
      const baselineConfig =
        target === "current"
          ? await api.updateDisplayBaseline(selectedDisplay.id)
          : await api.resetDisplayBaseline(selectedDisplay.id, target);
      const neutralProfile = neutralizeProfile({ ...cloneProfile(draft), targetDisplayId: selectedDisplay.id });
      const saved = neutralProfile.id ? await api.saveProfile(neutralProfile) : baselineConfig;
      await api.applyDraftProfile(neutralProfile);
      setConfig(saved);
      setDraft(neutralProfile);
      setDirtyDraft(false);
      setStatus("Baseline updated - current profile neutralized");
      setError(null);
      setBaselineDialogTarget(null);
      setIsBaselineMenuOpen(false);
    } catch (caught) {
      setError(String(caught));
    } finally {
      setIsBusy(false);
    }
  }

  function startRename(profile: Profile) {
    setEditingProfileId(profile.id);
    setEditingName(profile.name);
  }

  function commitRename() {
    const nextName = editingName.trim();
    if (!editingProfileId || !nextName) {
      setEditingProfileId(null);
      return;
    }

    updateDraft((profile) => (profile.id === editingProfileId ? { ...profile, name: nextName } : profile));
    setConfig((current) => ({
      ...current,
      profiles: current.profiles.map((profile) =>
        profile.id === editingProfileId ? { ...profile, name: nextName } : profile,
      ),
    }));
    setEditingProfileId(null);
  }

  return (
    <main className="app-shell">
      <aside className="profile-rail">
        <div className="rail-header">
          <div>
            <h1>GammaDeck</h1>
            <p>Profiles</p>
          </div>
          <button className="add-profile-button" onClick={createProfile} title="New profile" disabled={isBusy}>
            <Plus size={18} />
          </button>
        </div>

        <div className="profile-list">
          {profiles.map((profile) => (
            <ProfileRow
              key={profile.id}
              isActive={profile.id === draft.id}
              isEditing={editingProfileId === profile.id}
              isProtected={profile.id === DEFAULT_PROFILE_ID}
              name={profile.id === draft.id ? draft.name : profile.name}
              meta={profileMeta(profile)}
              editingName={editingName}
              onClick={() => selectProfile(profile)}
              onDoubleClick={() => startRename(profile)}
              onDelete={() => deleteProfile(profile.id)}
              onEditingNameChange={setEditingName}
              onCommitRename={commitRename}
            />
          ))}
        </div>
      </aside>

      <section className="editor-panel">
        <div className="top-fields">
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

          <label className="field">
            <span>Hotkey</span>
            <input
              className="hotkey-input"
              value={draft.hotkey ?? ""}
              placeholder="Click and press keys"
              onChange={(event) =>
                updateDraft((profile) => ({ ...profile, hotkey: event.target.value || null }))
              }
              onKeyDown={(event) => {
                if (event.key === "Escape") {
                  event.preventDefault();
                  updateDraft((profile) => ({ ...profile, hotkey: null }));
                  event.currentTarget.blur();
                  return;
                }

                const binding = formatShortcut(event);
                if (!binding) {
                  return;
                }
                event.preventDefault();
                updateDraft((profile) => ({ ...profile, hotkey: binding }));
                event.currentTarget.blur();
              }}
            />
          </label>

          <div className="mode-row" aria-label="Channel mode">
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
        </div>

        {visibleError ? <div className="error-banner">{visibleError}</div> : null}

        <div className="editor-core">
          {draft.channelMode === "linked" ? (
            <SliderGroup settings={settings} onChange={updateChannel} />
          ) : (
            <div className="rgb-grid">
              <ChannelColumn name="red" label="Red" settings={draft.red} onChange={updateChannel} />
              <ChannelColumn name="green" label="Green" settings={draft.green} onChange={updateChannel} />
              <ChannelColumn name="blue" label="Blue" settings={draft.blue} onChange={updateChannel} />
            </div>
          )}

          <div className="lut-block">
            <div className="lut-header">
              <span>LUT Preview</span>
              <span>{draft.channelMode === "linked" ? "Linked" : "RGB"}</span>
            </div>
            <LutPreview profile={draft} />
          </div>
        </div>

        <div className="bottom-actions">
          <button className="warn-button" onClick={resetProfile} disabled={isBusy || !draft.targetDisplayId}>
            <RotateCcw size={15} />
            Reset
          </button>
          <div className="baseline-actions">
            <button
              className="primary-button baseline-main-button"
              onClick={() => setBaselineDialogTarget("current")}
              disabled={isBusy || !selectedDisplay?.isSupported}
            >
              <Save size={15} />
              Baseline
            </button>
            <button
              className="primary-button baseline-menu-button"
              onClick={() => setIsBaselineMenuOpen((current) => !current)}
              disabled={isBusy || !selectedDisplay?.isSupported}
              title="Baseline reset options"
            >
              <ChevronDown size={14} />
            </button>
            {isBaselineMenuOpen ? (
              <div className="baseline-menu">
                <button type="button" onClick={() => setBaselineDialogTarget("initial")}>
                  Reset to first-run baseline
                </button>
                <button type="button" onClick={() => setBaselineDialogTarget("neutral")}>
                  Reset to neutral baseline
                </button>
              </div>
            ) : null}
          </div>
          <span className="status-line">{statusText(status, dirtyDraft, selectedDisplay)}</span>
        </div>
      </section>

      {baselineDialogTarget ? (
        <div className="modal-backdrop" role="presentation">
          <div className="modal-panel" role="dialog" aria-modal="true" aria-labelledby="baseline-title">
            <h2 id="baseline-title">{baselineDialogTitle(baselineDialogTarget)}</h2>
            {baselineDialogCopy(baselineDialogTarget).map((paragraph) => (
              <p key={paragraph}>{paragraph}</p>
            ))}
            <div className="modal-actions">
              <button className="ghost-button" onClick={() => setBaselineDialogTarget(null)} disabled={isBusy}>
                Cancel
              </button>
              <button
                className="warn-button"
                onClick={() => updateBaseline(baselineDialogTarget)}
                disabled={isBusy || !selectedDisplay?.isSupported}
              >
                {baselineDialogConfirmLabel(baselineDialogTarget)}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </main>
  );
}

function baselineDialogTitle(target: BaselineDialogTarget) {
  if (target === "initial") {
    return "Reset baseline to first-run";
  }

  if (target === "neutral") {
    return "Reset baseline to neutral";
  }

  return "Update baseline";
}

function baselineDialogConfirmLabel(target: BaselineDialogTarget) {
  if (target === "initial") {
    return "Reset to first-run";
  }

  if (target === "neutral") {
    return "Reset to neutral";
  }

  return "Update baseline";
}

function baselineDialogCopy(target: BaselineDialogTarget) {
  if (target === "initial") {
    return [
      "GammaDeck will replace the current baseline for this display with the first-run baseline captured when this app first recorded this display.",
      "This is meant to restore the original reference GammaDeck saved before later baseline updates. For upgraded older configs, GammaDeck uses the earliest existing saved baseline it can migrate.",
      "The current profile will be reset to neutral so it does not stack again. Other profiles for this display keep their saved values and will apply relative to the restored baseline.",
      "Because the reference point changes, you may need to manually readjust other profiles for this display.",
    ];
  }

  if (target === "neutral") {
    return [
      "GammaDeck will replace the current baseline for this display with a plain neutral ramp from 0 to 65535.",
      "This makes Gamma 1.00, Brightness 0.00, and Contrast 1.00 mean no extra LUT adjustment from GammaDeck, and may ignore Windows calibration or GPU control panel color changes.",
      "The current profile will be reset to neutral. Other profiles for this display keep their saved values and will apply relative to the neutral baseline.",
      "Because the reference point changes, you may need to manually readjust other profiles for this display.",
    ];
  }

  return [
    "GammaDeck will save the current display gamma ramp as the baseline reference for this display. After this, Gamma 1.00, Brightness 0.00, and Contrast 1.00 will mean this exact current display state.",
    "If a GammaDeck profile is currently applied, that visible effect will become part of the new baseline, and the current profile will be reset to neutral so it does not stack again.",
    "Other profiles for this display will keep their saved values and will be applied relative to the new baseline. The previous current baseline in GammaDeck will be replaced.",
    "Because the reference point changes, you may need to manually readjust other profiles for this display.",
  ];
}

function ProfileRow({
  isActive,
  isEditing,
  isProtected,
  name,
  meta,
  editingName,
  onClick,
  onDoubleClick,
  onDelete,
  onEditingNameChange,
  onCommitRename,
}: {
  isActive: boolean;
  isEditing: boolean;
  isProtected: boolean;
  name: string;
  meta: string;
  editingName: string;
  onClick: () => void;
  onDoubleClick: () => void;
  onDelete: () => void;
  onEditingNameChange: (value: string) => void;
  onCommitRename: () => void;
}) {
  return (
    <button
      className={isActive ? "profile-row active" : "profile-row"}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      type="button"
    >
      {isEditing ? (
        <input
          className="profile-name-input"
          value={editingName}
          autoFocus
          onChange={(event) => onEditingNameChange(event.target.value)}
          onBlur={onCommitRename}
          onClick={(event) => event.stopPropagation()}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              onCommitRename();
            }
            if (event.key === "Escape") {
              event.currentTarget.blur();
            }
          }}
        />
      ) : (
        <>
          <strong>{name}</strong>
          <span className="profile-meta">{meta}</span>
        </>
      )}
      {!isProtected ? (
        <span
          className="delete-profile-button"
          role="button"
          tabIndex={0}
          title="Delete profile"
          onClick={(event) => {
            event.stopPropagation();
            onDelete();
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              event.stopPropagation();
              onDelete();
            }
          }}
        >
          <TrashIcon />
        </span>
      ) : null}
    </button>
  );
}

function TrashIcon() {
  return (
    <svg className="trash-icon" viewBox="0 0 16 16" aria-hidden="true">
      <path d="M5.25 2.75h5.5l.55 1.5h2.2v1.5h-11v-1.5h2.2l.55-1.5Z" />
      <path
        d="M4.2 6.4h7.6l-.45 6.1a1.35 1.35 0 0 1-1.35 1.25H6a1.35 1.35 0 0 1-1.35-1.25L4.2 6.4Z"
        fill="none"
        stroke="currentColor"
        strokeLinejoin="round"
        strokeWidth="1.35"
      />
      <path d="M6.75 7.85v4.05M9.25 7.85v4.05" fill="none" stroke="currentColor" strokeLinecap="round" strokeWidth="1.2" />
    </svg>
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
      <RangeRow label="Gamma" min={0.25} max={2.5} step={0.01} value={settings.gamma} onChange={(value) => onChange("gamma", value)} />
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
      <RangeRow label="Gamma" min={0.25} max={2.5} step={0.01} value={settings.gamma} onChange={(value) => onChange("gamma", value, name)} />
      <RangeRow label="Brightness" min={-0.35} max={0.35} step={0.01} value={settings.brightness} onChange={(value) => onChange("brightness", value, name)} />
      <RangeRow label="Contrast" min={0.5} max={1.75} step={0.01} value={settings.contrast} onChange={(value) => onChange("contrast", value, name)} />
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
      <span className="range-label">{label}</span>
      <span className="range-control">
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={(event) => onChange(Number(event.target.value))}
        />
        <NumberInput
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={onChange}
        />
      </span>
    </label>
  );
}

function NumberInput({
  min,
  max,
  step,
  value,
  onChange,
}: {
  min: number;
  max: number;
  step: number;
  value: number;
  onChange: (value: number) => void;
}) {
  const [text, setText] = useState(formatNumber(value, step));
  const [isEditing, setIsEditing] = useState(false);
  const skipNextBlurCommitRef = useRef(false);

  useEffect(() => {
    if (!isEditing) {
      setText(formatNumber(value, step));
    }
  }, [isEditing, step, value]);

  function commit(nextText = text) {
    const parsed = parseNumberInput(nextText);
    const nextValue = parsed === null ? value : clamp(roundToStep(parsed, step), min, max);
    setIsEditing(false);
    setText(formatNumber(nextValue, step));
    if (nextValue !== value) {
      onChange(nextValue);
    }
  }

  function revert() {
    setIsEditing(false);
    setText(formatNumber(value, step));
  }

  function nudge(direction: 1 | -1, multiplier: number) {
    const nextValue = clamp(roundToStep(value + direction * step * multiplier, step), min, max);
    setText(formatNumber(nextValue, step));
    if (nextValue !== value) {
      onChange(nextValue);
    }
  }

  return (
    <input
      className="number-input"
      type="text"
      inputMode="decimal"
      value={text}
      aria-label={`${min} to ${max}`}
      onFocus={(event) => {
        setIsEditing(true);
        event.currentTarget.select();
      }}
      onChange={(event) => {
        const nextText = event.target.value;
        setText(nextText);

        const parsed = parseNumberInput(nextText);
        if (parsed === null || parsed < min || parsed > max) {
          return;
        }

        const nextValue = roundToStep(parsed, step);
        if (nextValue !== value) {
          onChange(nextValue);
        }
      }}
      onBlur={() => {
        if (skipNextBlurCommitRef.current) {
          skipNextBlurCommitRef.current = false;
          return;
        }

        commit();
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          commit();
          event.currentTarget.blur();
          return;
        }

        if (event.key === "Escape") {
          event.preventDefault();
          skipNextBlurCommitRef.current = true;
          revert();
          event.currentTarget.blur();
          return;
        }

        if (event.key === "ArrowUp" || event.key === "ArrowDown") {
          event.preventDefault();
          const multiplier = event.shiftKey ? 10 : 1;
          nudge(event.key === "ArrowUp" ? 1 : -1, multiplier);
        }
      }}
    />
  );
}

function parseNumberInput(text: string) {
  const trimmed = text.trim();
  if (!trimmed || trimmed === "-" || trimmed === "." || trimmed === "-.") {
    return null;
  }

  if (!/^-?(?:\d+|\d*\.\d+|\d+\.)$/.test(trimmed)) {
    return null;
  }

  const value = Number(trimmed);
  return Number.isFinite(value) ? value : null;
}

function formatNumber(value: number, step: number) {
  return value.toFixed(decimalPlaces(step));
}

function roundToStep(value: number, step: number) {
  const factor = 10 ** decimalPlaces(step);
  return Math.round(value * factor) / factor;
}

function decimalPlaces(value: number) {
  const [, fraction = ""] = value.toString().split(".");
  return fraction.length;
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
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
    <svg className="lut-preview" viewBox="0 0 168 168" role="img" aria-label="LUT curve preview">
      <path className="grid-line" d="M8 8 H160 M8 46 H160 M8 84 H160 M8 122 H160 M8 160 H160" />
      <path className="grid-line" d="M8 8 V160 M46 8 V160 M84 8 V160 M122 8 V160 M160 8 V160" />
      {channels.map((channel) => (
        <path key={channel.className} className={`curve ${channel.className}`} d={curvePath(channel.settings)} />
      ))}
    </svg>
  );
}

function curvePath(settings: ChannelSettings) {
  const points = Array.from({ length: 32 }, (_, index) => {
    const x = index / 31;
    const gamma = Math.min(2.5, Math.max(0.25, settings.gamma));
    const brightness = Math.min(0.35, Math.max(-0.35, settings.brightness));
    const contrast = Math.min(1.75, Math.max(0.5, settings.contrast));
    const y = Math.min(1, Math.max(0, (Math.pow(x, 1 / gamma) - 0.5) * contrast + 0.5 + brightness));
    return [8 + x * 152, 160 - y * 152];
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

function profileMeta(profile: Profile) {
  return profile.hotkey ?? "No hotkey";
}

function statusText(status: string, dirtyDraft: boolean, selectedDisplay: DisplayInfo | null) {
  if (!selectedDisplay?.isSupported) {
    return "Esc clears hotkey · Unsupported";
  }

  if (dirtyDraft) {
    return "Auto-saving changes";
  }

  return status === "Ready" ? "Esc clears hotkey" : status;
}

function isUnsupportedGammaError(message: string) {
  return message.toLowerCase().includes("gamma control is unsupported");
}

function devLog(message: string, data?: unknown) {
  const env = (import.meta as unknown as { env?: { DEV?: boolean } }).env;
  if (env?.DEV) {
    console.info(`[GammaDeck] ${message}`, data);
  }
}
