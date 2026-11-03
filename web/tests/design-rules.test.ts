import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";

interface Rule {
    name: string;
    why: string;
    pattern: RegExp;
    /** A line the rule must flag; proves the pattern still works. */
    bad: string;
    /** A line the rule must allow. */
    good: string;
}

/** The mechanical half of the design rules in CLAUDE.md. Each pattern runs on every line of src/ and index.html. */
const rules: Rule[] = [
    {
        name: "all caps text",
        why: "Never set text in capitals, by CSS or by Tailwind. Write labels in sentence case.",
        pattern:
            /text-transform\s*:\s*(uppercase|capitalize)|textTransform\s*:\s*["'](uppercase|capitalize)|(^|[\s"'`])(uppercase|capitalize|normal-case)([\s"'`]|$)|font-variant(-caps)?\s*:\s*[^;]*(small-caps|all-small-caps|titling-caps)/,
        bad: "label { text-transform: uppercase; }",
        good: "label { text-transform: none; }",
    },
    {
        name: "gradients",
        why: "No gradient washes, gradient text, or gradient buttons. Use flat colors from tokens.css.",
        pattern: /-gradient\s*\(|background-clip\s*:\s*text|bg-clip-text|(^|[\s"'`:])(bg|from|via|to)-gradient/,
        bad: "background: linear-gradient(135deg, #6366f1, #a855f7);",
        good: "background: var(--surface);",
    },
    {
        name: "glassmorphism",
        why: "No frosted-glass panels.",
        pattern: /backdrop-filter|backdrop-blur/,
        bad: "backdrop-filter: blur(12px);",
        good: "background: var(--surface);",
    },
    {
        name: "glow",
        why: "No glowing shadows or neon accents.",
        pattern: /drop-shadow\s*\(|(glow|neon)/i,
        bad: "box-shadow: 0 0 24px var(--glow);",
        good: "border: 1px solid var(--border);",
    },
    {
        name: "hover lift",
        why: "Cards do not rise on hover. Show state with a border, color, or outline change.",
        pattern: /:hover[^{]*\{[^}]*translateY\(\s*-|hover:-translate-y|hover:scale-/,
        bad: ".card:hover { transform: translateY(-4px); }",
        good: ".card:hover { border-color: var(--accent); }",
    },
    {
        name: "emoji",
        why: "No emoji as icons or decoration. Draw tiles and icons as SVG.",
        pattern: /\p{Extended_Pictographic}/u,
        bad: "<h2>Your hand 🎉</h2>",
        good: "<h2>Your hand</h2>",
    },
    {
        name: "template punctuation in copy",
        why: "No arrows on links or buttons, middle-dot meta strings, or spaced em dashes in labels.",
        pattern: /→|\w · \w| — |&rarr;|&middot;|&mdash;/,
        bad: "<a>See the rules →</a>",
        good: "<a>See the rules</a>",
    },
    {
        name: "marketing filler",
        why: "Say what the thing does. No hype words.",
        pattern:
            /\b(seamless(ly)?|elevate[sd]?|unleash(es|ed)?|supercharge[sd]?|next-level|cutting-edge|revolutioni[sz]e[sd]?|effortless(ly)?|game-chang(er|ing)|unlock the power)\b/i,
        bad: "<p>Elevate your game with seamless play.</p>",
        good: "<p>Play a hand against three bots.</p>",
    },
];

function sourceFiles(dir: string): string[] {
    return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
        const child = path.join(dir, entry.name);

        if (entry.isDirectory()) {
            return sourceFiles(child);
        }

        return /\.(css|ts|tsx|html)$/.test(entry.name) ? [child] : [];
    });
}

test("every design rule flags its bad example and allows its good one", () => {
    for (const rule of rules) {
        assert.match(rule.bad, rule.pattern, `${rule.name} no longer flags: ${rule.bad}`);
        assert.doesNotMatch(rule.good, rule.pattern, `${rule.name} wrongly flags: ${rule.good}`);
    }
});

test("src and index.html follow the design rules", () => {
    const root = new URL("../", import.meta.url).pathname;
    const files = [...sourceFiles(path.join(root, "src")), path.join(root, "index.html")];
    const violations = files.flatMap((file) =>
        readFileSync(file, "utf8")
            .split("\n")
            .flatMap((line, index) =>
                rules
                    .filter((rule) => rule.pattern.test(line))
                    .map((rule) => `${file.slice(root.length)}:${index + 1} ${rule.name}: ${rule.why}`),
            ),
    );

    assert.deepEqual(violations, []);
});

test("CSS blocks that contain a hover rule do not lift the element", () => {
    const root = new URL("../", import.meta.url).pathname;
    const css = sourceFiles(path.join(root, "src"))
        .filter((file) => file.endsWith(".css"))
        .map((file) => readFileSync(file, "utf8").replaceAll("\n", " "));

    assert.deepEqual(
        css.filter((text) => rules.find((rule) => rule.name === "hover lift")!.pattern.test(text)),
        [],
    );
});
