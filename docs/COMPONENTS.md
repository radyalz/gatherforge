# Component Rules

src/components/mod.rs defines reusable UI primitives.

- heading: page title and explanatory subtitle.
- panel: consistent visual container.
- metric: dashboard KPI card.
- nav: sidebar navigation control.
- action: primary action button.

Keep processing logic separate from presentation logic.

UI components should not directly delete, move or overwrite files.
Destructive operations require explicit confirmation.
