//! The whole stylesheet, inlined.
//!
//! No webfonts and no CDN: this file has to open on a machine that has never heard of
//! this application, possibly with no network, possibly in ten years. System stacks
//! only — a serif for reading and a monospace for anything with a date in it, which is
//! the same split the app makes and the same one a printed gazetteer makes.
//!
//! It carries a dark scheme as well, because a document handed to somebody else is read
//! on their machine and in their settings, not the writer's.

pub const CSS: &str = r#"
:root {
  --paper: #fbfaf7;
  --ink: #1c1a17;
  --ink-2: #4a453e;
  --ink-3: #7d766b;
  --rule: #ddd8cd;
  --accent: #7a5c2e;
  --sea: #cfd9de;
  --f-serif: "Iowan Old Style", "Palatino Linotype", Palatino, Georgia, "Times New Roman", serif;
  --f-mono: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
}

@media (prefers-color-scheme: dark) {
  :root {
    --paper: #171614;
    --ink: #e9e4d9;
    --ink-2: #bab3a4;
    --ink-3: #8b8478;
    --rule: #33302a;
    --accent: #c9a35f;
    --sea: #222b31;
  }
}

* { box-sizing: border-box; }

body {
  margin: 0;
  padding: 0 24px 96px;
  background: var(--paper);
  color: var(--ink);
  font-family: var(--f-serif);
  font-size: 17px;
  line-height: 1.62;
  -webkit-text-size-adjust: 100%;
}

main { max-width: 46rem; margin: 0 auto; }
.wide { max-width: 62rem; margin: 0 auto; }

header.world { padding: 72px 0 28px; border-bottom: 1px solid var(--rule); }
header.world h1 { margin: 0; font-size: 2.6rem; line-height: 1.1; letter-spacing: -0.015em; text-wrap: balance; }
header.world .scope { margin: 14px 0 0; font-family: var(--f-mono); font-size: 0.72rem; letter-spacing: 0.1em; text-transform: uppercase; color: var(--ink-3); }

h2 { margin: 64px 0 4px; font-size: 1.45rem; letter-spacing: -0.01em; }
h2.section { padding-bottom: 6px; border-bottom: 1px solid var(--rule); }
h3 { margin: 40px 0 2px; font-size: 1.2rem; }
p { margin: 0 0 1em; }
a { color: var(--accent); text-decoration-thickness: 1px; text-underline-offset: 2px; }

figure { margin: 32px 0; }
figure svg { display: block; width: 100%; height: auto; background: var(--sea); border: 1px solid var(--rule); }
figcaption { margin-top: 8px; font-family: var(--f-mono); font-size: 0.72rem; color: var(--ink-3); }

.contents { columns: 2; column-gap: 32px; margin: 20px 0 0; padding: 0; list-style: none; font-size: 0.95rem; }
.contents li { break-inside: avoid; }
.contents .kind { font-family: var(--f-mono); font-size: 0.68rem; letter-spacing: 0.1em; text-transform: uppercase; color: var(--ink-3); }

article { padding-top: 8px; scroll-margin-top: 16px; }
article .meta { margin: 2px 0 12px; font-family: var(--f-mono); font-size: 0.75rem; color: var(--ink-3); }
article .aka { font-style: italic; color: var(--ink-2); }

.facts { width: 100%; border-collapse: collapse; margin: 0 0 18px; font-size: 0.86rem; }
.facts th { text-align: left; font-family: var(--f-mono); font-size: 0.66rem; letter-spacing: 0.11em; text-transform: uppercase; font-weight: 400; color: var(--ink-3); padding: 0 12px 5px 0; border-bottom: 1px solid var(--rule); }
.facts td { padding: 5px 12px 5px 0; border-bottom: 1px solid var(--rule); vertical-align: top; }
.facts .when { font-family: var(--f-mono); font-size: 0.78rem; color: var(--ink-2); white-space: nowrap; font-variant-numeric: tabular-nums; }

.scroll { overflow-x: auto; }
blockquote { margin: 0 0 1em; padding-left: 16px; border-left: 2px solid var(--rule); color: var(--ink-2); }
code { font-family: var(--f-mono); font-size: 0.85em; }

footer { max-width: 46rem; margin: 96px auto 0; padding-top: 20px; border-top: 1px solid var(--rule); font-family: var(--f-mono); font-size: 0.72rem; line-height: 1.7; color: var(--ink-3); }

@media (max-width: 620px) {
  body { font-size: 16px; }
  .contents { columns: 1; }
  header.world { padding-top: 44px; }
}

@media print {
  body { background: #fff; color: #000; }
  article { break-inside: avoid-page; }
}
"#;
