import { tileName } from "./names";
import type { Action } from "@mjers/mjers/types";

export function actionLabel(action: Action, offered?: string): string {
    if (typeof action === "string") {
        return { self_draw_win: "Win by self-draw", pass: "Pass", pong: "Pong", kong: "Gong", win: `Win with ${offered ?? "the discard"}` }[
            action
        ];
    }

    if ("discard" in action) {
        return `Discard ${tileName(action.discard)}`;
    }

    if ("chow" in action) {
        return `Seung from ${tileName(action.chow)}`;
    }

    if ("concealed_kong" in action) {
        return `Am gong of ${tileName(action.concealed_kong)}`;
    }

    return `Gong with ${tileName(action.upgrade_kong)}`;
}
