const scaffoldSections = [
  {
    title: "GGPK parser",
    body: "Rust crates will own archive traversal, raw file extraction, and patch automation.",
  },
  {
    title: "Layout model",
    body: "FlatBuffers is wired through build.rs; the checked-in schema is intentionally minimal.",
  },
  {
    title: "Visualizer",
    body: "React owns app chrome, while Pixi.js is reserved for the eventual layout canvas.",
  },
  {
    title: "Tauri shell",
    body: "The desktop app is scaffolded so data can stay local as the scraper takes shape.",
  },
];

export function App() {
  return (
    <main className="shell">
      <section className="hero">
        <p className="eyebrow">PoE1 Acts 1-5</p>
        <h1>Layout Visualizer Scaffold</h1>
        <p>
          This commit only establishes the project shape. The next commits can
          fill in GGPK parsing, extraction, data modeling, and Pixi rendering
          once the real campaign data is understood.
        </p>
      </section>
      <section className="lanes" aria-label="Scaffolded work areas">
        {scaffoldSections.map((section) => (
          <article key={section.title}>
            <h2>{section.title}</h2>
            <p>{section.body}</p>
          </article>
        ))}
      </section>
    </main>
  );
}
