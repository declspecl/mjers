import type { GradedMove } from "../grading/use-grader";
import { useEffect, useState } from "react";

const SHOWN_MS = 4200;

/** A brief word on your last move, shown once the engine has judged it. */
export function GradeToast({ latest }: { latest: GradedMove | undefined }) {
    const [visible, setVisible] = useState<GradedMove>();

    useEffect(() => {
        if (!latest) {
            setVisible(undefined);

            return;
        }

        setVisible(latest);

        const timer = setTimeout(() => setVisible(undefined), SHOWN_MS);

        return () => clearTimeout(timer);
    }, [latest]);

    if (!visible) {
        return null;
    }

    return (
        <p
            className="grade-toast"
            role="status"
            data-grade={visible.result.grade.toLowerCase()}
            key={`${visible.hand}-${visible.decision}`}
        >
            <strong>
                {visible.result.grade}
                {visible.result.basis ? `: ${visible.result.basis.toLowerCase()}` : ""}
            </strong>
            {visible.result.summary && <span className="grade-explanation">{visible.result.summary}</span>}
        </p>
    );
}
