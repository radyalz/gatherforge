# GatherForge Forge Button Lab (real React + Tailwind + shadcn pattern)

This is **an independently runnable design lab**, not a replacement Rune frontend and **not embedded** in the Rune process. It demonstrates the identical 4px/rim/orbit/ripple interaction used by the native Rune implementation.

```sh
cd ui/shadcn
npm install
npm run dev
```

Uses `@radix-ui/react-slot`, `class-variance-authority`, `tailwindcss` v4, and a source-owned button component following the shadcn architecture. The component lives at `src/components/ui/button.tsx`. The moving edge and ripple are in `src/styles.css`.

GPUI and egui are separate native event-loop/rendering frameworks. They cannot be imported like React components into one Rune widget tree. Rune stays the actual application runtime in this package. If a GPUI frontend is desired, it must be a separate frontend migration or independent process sharing the backend.
