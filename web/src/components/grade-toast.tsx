import type { GradedMove } from "../grading/use-grader";
import { useEffect, useState } from "react";

const SHOWN_MS = 2600;

/** A brief word on your last move, shown once the engine has judged it. */
export function GradeToast({ latest }: { latest: GradedMove | undefined }) {
    const [visible, setVisible] = useState<GradedMove>();

    useEffect(() => {
        if (!latest) {
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
            {visible.result.grade}
        </p>
    );
}
