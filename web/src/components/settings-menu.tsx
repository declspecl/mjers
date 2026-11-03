import type { Settings } from "../engine/settings";
import { useEffect, useRef, useState } from "react";

interface SettingsMenuProps {
    settings: Settings;
    onChange: (change: Partial<Settings>) => void;
    onSkipOpening?: () => void;
}

/** One small control in the corner: the view, move feedback, and the opening sequence. */
export function SettingsMenu({ settings, onChange, onSkipOpening }: SettingsMenuProps) {
    const [open, setOpen] = useState(false);
    const panel = useRef<HTMLDivElement>(null);

    useEffect(() => {
        if (!open) {
            return;
        }

        const close = (event: PointerEvent | KeyboardEvent) => {
            if (event instanceof KeyboardEvent ? event.key === "Escape" : !panel.current?.contains(event.target as Node)) {
                setOpen(false);
            }
        };

        document.addEventListener("pointerdown", close);
        document.addEventListener("keydown", close);

        return () => {
            document.removeEventListener("pointerdown", close);
            document.removeEventListener("keydown", close);
        };
    }, [open]);

    return (
        <div className="settings" ref={panel}>
            {onSkipOpening && (
                <button type="button" className="quiet" onClick={onSkipOpening}>
                    Skip to the deal
                </button>
            )}
            <button
                type="button"
                className="quiet icon"
                aria-expanded={open}
                aria-label="Table settings"
                onClick={() => setOpen((value) => !value)}
            >
                <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M4 7h10M18 7h2M4 17h4M12 17h8" />
                    <circle cx="16" cy="7" r="2" />
                    <circle cx="10" cy="17" r="2" />
                </svg>
            </button>
            {open && (
                <div className="settings-panel" role="group" aria-label="Table settings">
                    <fieldset>
                        <legend>View</legend>
                        <label>
                            <input
                                type="radio"
                                name="view"
                                checked={settings.view === "immersive"}
                                onChange={() => onChange({ view: "immersive" })}
                            />
                            Immersive: the table only
                        </label>
                        <label>
                            <input
                                type="radio"
                                name="view"
                                checked={settings.view === "analytic"}
                                onChange={() => onChange({ view: "analytic" })}
                            />
                            Analytic: status, hints and history
                        </label>
                    </fieldset>
                    <label>
                        <input
                            type="checkbox"
                            checked={settings.feedback}
                            onChange={(event) => onChange({ feedback: event.target.checked })}
                        />
                        Grade my moves
                    </label>
                    <label>
                        <input
                            type="checkbox"
                            checked={settings.opening}
                            onChange={(event) => onChange({ opening: event.target.checked })}
                        />
                        Show the shuffle and deal
                    </label>
                    <p className="settings-note">Press V to switch views.</p>
                </div>
            )}
        </div>
    );
}
